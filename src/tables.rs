use crate::{
    PageId,
    buffer_pool::BufferPool,
    error::{DbError, DbInputError, DbResult},
    page::{
        PageAccessor, PageAccessorMut, PageHeaderReader, PageKind, SlottedPageMut,
        fsm::{FreeSpaceMapper, FreeSpaceMapperMut},
    },
    value::{DataType, Value},
};

#[allow(dead_code)]
#[derive(Debug)]
pub struct RecordId {
    page: u64,
    slot: u16,
}

#[allow(dead_code)]
pub struct Record {
    rid: RecordId,
    data: Vec<u8>,
}

pub struct Tuple {
    pub values: Vec<Value>,
}
impl Tuple {
    pub fn new(values: &[Value]) -> Self {
        Self {
            values: values.to_vec(),
        }
    }

    fn calc_header_size(&self) -> usize {
        let mut size = 0;
        for value in &self.values {
            match value {
                // (offset as u16, len as u16)
                Value::VarChar(_) | Value::Blob(_) => {
                    size += 4;
                }
                // value types
                _ => {
                    size += value.size();
                }
            }
        }
        size
    }

    fn gen_null_bitmap(&self) -> Vec<u8> {
        let num_bytes = self.values.len().div_ceil(8);
        let mut bytes = vec![0u8; num_bytes];

        for (idx, v) in self.values.iter().enumerate() {
            if matches!(v, Value::Null) {
                let byte = idx / 8;
                let shamt = (7 - (idx % 8)) as u8;
                bytes[byte] |= 1u8 << shamt;
            }
        }
        bytes
    }

    pub fn serialize(&self, schema: &TableSchema) -> Vec<u8> {
        let mut header_bytes = Vec::new();
        let mut variable_bytes = Vec::new();

        let mut variable_byte_offset = self.calc_header_size();

        let null_bitmap = self.gen_null_bitmap();
        header_bytes.extend_from_slice(&null_bitmap);

        // Put the fixed-width values in the header and variable length ones in the variable_bytes
        // vec, track how big they are in the `offset` variable to put (offset, length) pairs in the header.
        for (val, col) in self.values.iter().zip(schema.attributes.iter()) {
            match val {
                Value::Int(i) => {
                    header_bytes.extend_from_slice(&i.to_be_bytes());
                }
                Value::Float(f) => {
                    header_bytes.extend_from_slice(&f.to_be_bytes());
                }
                Value::Boolean(b) => {
                    if *b {
                        header_bytes.push(1u8);
                    } else {
                        header_bytes.push(0u8);
                    }
                }
                Value::VarChar(s) => {
                    let len = s.len();
                    header_bytes.extend_from_slice(&(variable_byte_offset as u16).to_be_bytes());
                    header_bytes.extend_from_slice(&(len as u16).to_be_bytes());
                    variable_bytes.extend_from_slice(s.as_bytes());
                    variable_byte_offset += len;
                }
                Value::Blob(b) => {
                    let len = b.len();
                    header_bytes.extend_from_slice(&(variable_byte_offset as u16).to_be_bytes());
                    header_bytes.extend_from_slice(&(len as u16).to_be_bytes());
                    variable_bytes.extend_from_slice(b);
                    variable_byte_offset += len;
                }
                Value::Null => {
                    // TODO: Can I do nothing here with a null bitmap? Probably not...
                    // Do I just fill with the DT size so all headers are the same length? Probably, but consult textbook.
                    header_bytes.extend_from_slice(&vec![0u8; col.data_type.size()]);
                }
            }
        }

        header_bytes.extend(&variable_bytes);
        header_bytes
    }
}

#[derive(Debug, Clone)]
#[repr(C)]
pub struct ColumnDefinition {
    pub name: String,
    pub data_type: DataType,
    pub is_key: bool,
    pub is_nullable: bool,
}
impl ColumnDefinition {
    pub fn new(
        name: String,
        data_type: DataType,
        is_key: bool,
        is_nullable: bool,
    ) -> DbResult<Self> {
        if name.len() > u8::MAX as usize {
            return Err(DbError::InputError(DbInputError::StringTooLong));
        }
        Ok(Self {
            name,
            data_type,
            is_key,
            is_nullable,
        })
    }
    pub fn to_be_bytes(&self) -> DbResult<Vec<u8>> {
        let mut bytes = vec![];
        let name_len = u8::try_from(self.name.len())?;
        bytes.extend_from_slice(&name_len.to_be_bytes());
        bytes.extend_from_slice(self.name.as_bytes());
        bytes.push(self.data_type as u8);
        bytes.push(u8::from(self.is_key));
        bytes.push(u8::from(self.is_nullable));
        Ok(bytes)
    }

    pub fn from_be_bytes(bytes: &[u8]) -> DbResult<(Self, usize)> {
        let mut data = bytes;
        let mut len = 4;
        let name_len = data[0] as usize;
        data = &data[1..];
        let name = String::from_utf8_lossy(&data[..name_len]);
        len += name_len;
        data = &data[name_len..];
        let data_type = DataType::from_u8(data[0]);
        data = &data[1..];
        let is_key = match data[0] {
            0 => false,
            1 => true,
            _ => {
                eprintln!("Casting {} to a bool?", data[0]);
                unreachable!()
            }
        };
        data = &data[1..];
        let is_nullable = match data[0] {
            0 => false,
            1 => true,
            _ => {
                eprintln!("Casting {} to a bool?", data[0]);
                unreachable!()
            }
        };
        Ok((
            Self {
                name: name.to_string(),
                data_type,
                is_key,
                is_nullable,
            },
            len,
        ))
    }
}

#[derive(Debug, Clone)]
pub struct TableSchema {
    pub attributes: Vec<ColumnDefinition>,
}
impl TableSchema {
    #[must_use]
    pub fn new(attributes: &[ColumnDefinition]) -> Self {
        Self {
            attributes: attributes.to_vec(),
        }
    }
    pub fn to_be_bytes(&self) -> DbResult<Vec<u8>> {
        let mut bytes = vec![];

        let num_attrs = u32::try_from(self.attributes.len())?;
        bytes.extend_from_slice(&num_attrs.to_be_bytes());
        for attr in &self.attributes {
            let attr_bytes = attr.to_be_bytes()?;
            bytes.extend_from_slice(&attr_bytes);
        }
        Ok(bytes)
    }
    pub fn from_be_bytes(bytes: &[u8]) -> DbResult<Self> {
        let mut data = bytes;

        let num_attrs = u32::from_be_bytes(bytes[0..4].try_into().unwrap());
        data = &data[4..];
        let mut attrs = Vec::new();
        tracing::debug!("Serializing {} attrs from {:?}", num_attrs, data);
        for _ in 0..num_attrs {
            let (attr, len) = ColumnDefinition::from_be_bytes(data)?;
            data = &data[len..];
            attrs.push(attr);
        }
        Ok(Self { attributes: attrs })
    }
}

const FIRST_FSM_PAGE: u64 = 1;

pub struct HeapStorage<'a> {
    table: &'a mut Table,
    bp: &'a mut BufferPool,
}

impl<'a> HeapStorage<'a> {
    pub fn new(table: &'a mut Table, bp: &'a mut BufferPool) -> Self {
        Self { table, bp }
    }

    fn find_page_with_free_space(&mut self) -> DbResult<u64> {
        let ffp = {
            let fsm = self.bp.get_page(PageId {
                file_id: self.table.id,
                page_num: self.table.current_fsm_idx,
            })?;

            let fsm_page = fsm.as_fsm()?;
            fsm_page.find_first_free_page(self.table.last_known_free_page)
        };

        self.table.last_known_free_page = ffp;
        Ok(ffp)
    }

    fn set_fsm_page_full(&mut self, page_num: u64) -> DbResult<()> {
        let mut fsm = self.bp.get_page(PageId {
            file_id: self.table.id,
            page_num: self.table.current_fsm_idx,
        })?;
        let mut fsm_page = fsm.as_fsm_mut()?;
        fsm_page.set_page_full(page_num);
        Ok(())
    }

    fn is_fsm_full(&mut self) -> DbResult<bool> {
        let full = {
            let fsm = self.bp.get_page(PageId {
                file_id: self.table.id,
                page_num: self.table.current_fsm_idx,
            })?;

            let fsm_page = fsm.as_fsm()?;
            fsm_page.is_full()
        };

        Ok(full)
    }

    fn handle_full_page(&mut self, page_num: u64) -> DbResult<()> {
        tracing::warn!("Setting page {page_num} full");
        self.set_fsm_page_full(page_num)?;
        let is_fsm_full = self.is_fsm_full()?;

        if is_fsm_full {
            tracing::warn!("FSM full!");
            let new_fsm_num = {
                let mut new_fsm_page =
                    self.bp.create_page(self.table.id, PageKind::FreeSpaceMap)?;
                let new_fsm = new_fsm_page.as_fsm_mut()?;
                new_fsm.header().page_id()
            };

            let mut old_fsm = self.bp.get_page(PageId {
                file_id: self.table.id,
                page_num: self.table.current_fsm_idx,
            })?;
            let mut old_fsm_page = old_fsm.as_fsm_mut()?;
            old_fsm_page.header_mut().set_next_page(new_fsm_num);
            self.table.current_fsm_idx = new_fsm_num;

            tracing::warn!(
                "FSM full! Created new one at {:?}",
                self.table.current_fsm_idx
            );
        }
        Ok(())
    }

    fn try_insert_into_page(&mut self, page_num: u64, record: &[u8]) -> DbResult<Option<RecordId>> {
        let mut page = if let Ok(page) = self.bp.get_page(PageId {
            file_id: self.table.id,
            page_num,
        }) {
            page
        } else {
            self.bp.create_page(self.table.id, PageKind::Heap)?
        };

        let rid = page.with_heap_mut(|heap| match heap.insert(record) {
            Ok(slot) => Ok(Some(RecordId {
                page: page_num,
                slot: slot.slot,
            })),
            Err(_) => Ok(None),
        })?;

        Ok(rid)
    }

    pub fn insert_record(&mut self, record: &[u8]) -> DbResult<RecordId> {
        tracing::debug!("Inserting record: {:?}", record);

        if let Some(page_num) = self.table.current_heap_page {
            match self.try_insert_into_page(page_num, record)? {
                Some(rid) => return Ok(rid),
                None => {
                    self.set_fsm_page_full(page_num)?;
                    self.table.current_heap_page = None;
                }
            }
        }

        let mut free_page = {
            let mut pnum = self.find_page_with_free_space()?;
            if pnum == u64::MAX {
                let new_page = self.bp.create_page(self.table.id, PageKind::Heap)?;
                self.table.current_heap_page = Some(new_page.page_id.page_num);
                pnum = new_page.page_id.page_num;
                tracing::warn!("Created page {pnum}");
            }
            pnum
        };

        loop {
            match self.try_insert_into_page(free_page, record)? {
                Some(rid) => return Ok(rid),
                None => {
                    self.handle_full_page(free_page)?;
                    self.table.current_heap_page = None;
                    free_page = self.find_page_with_free_space()?;
                }
            }
        }
    }
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct Table {
    pub id: u32,
    name: String,
    schema: TableSchema,
    current_fsm_idx: u64,
    last_known_free_page: u64,
    current_heap_page: Option<u64>,
}
impl Table {
    pub fn new(
        name: &str,
        schema: &TableSchema,
        bp: &mut BufferPool,
        file_id: u32,
    ) -> DbResult<Self> {
        // 2. Initialize the page file.
        // 2.1 Catalog file initialization (write schema).
        let num_entries = {
            let page = bp.get_page(PageId {
                file_id,
                page_num: 0,
            })?;
            let page = page.as_catalog()?;
            tracing::debug!("{:?}", page.header().num_entries());
            page.header().num_entries()
        };
        if num_entries == 0 {
            {
                let mut page = bp.get_page(PageId {
                    file_id,
                    page_num: 0,
                })?;
                let mut page = page.as_catalog_mut()?;
                page.insert(&schema.to_be_bytes()?)?;
            }
            // 2.2 Write free space map page.
            {
                let mut page = bp.create_page(file_id, PageKind::FreeSpaceMap)?;
                let mut page = page.as_fsm_mut()?;
                page.set_fsm_num(0);
                if page.header().page_id() != 1 {
                    return Err(DbError::CorruptPageFile);
                }
            }
            // 2.3 Make the first heap page for storage
            {
                let mut page = bp.create_page(file_id, PageKind::Heap)?;
                //assert!(page.handle.page_id.page_num == 2);
                let page = page.as_heap_mut()?;
                if page.header().page_id() != 2 {
                    return Err(DbError::CorruptPageFile);
                }
            }
        }

        Ok(Self {
            name: name.to_string(),
            id: file_id,
            schema: schema.clone(),
            current_fsm_idx: FIRST_FSM_PAGE,
            last_known_free_page: 2,
            current_heap_page: None,
        })
    }

    pub fn open(id: u32, name: &str, schema: &TableSchema) -> Self {
        Self {
            id,
            name: name.to_string(),
            schema: schema.clone(),
            current_fsm_idx: FIRST_FSM_PAGE,
            last_known_free_page: 2,
            current_heap_page: None,
        }
    }

    pub fn insert(&mut self, record: &[u8], bp: &mut BufferPool) -> DbResult<RecordId> {
        let mut storage = HeapStorage::new(self, bp);
        storage.insert_record(record)
    }
}
