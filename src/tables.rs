use anyhow::{Context, Result};
use parking_lot::RwLock;
<<<<<<< HEAD
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
=======
use std::sync::{Arc, atomic::{AtomicU64, Ordering}};
>>>>>>> main

use crate::{
    buffer_pool::BufferPool,
    catalog::CatalogManager,
    page::{
        fsm::{FreeSpaceMapper, FreeSpaceMapperMut},
        PageAccessor, PageAccessorMut, PageHeaderReader, PageKind, SlottedPageMut,
    },
    DbError, DbInputError, DbResult, PageId,
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
                let byte = (idx / 8) as usize;
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
                        header_bytes.push(1 as u8);
                    } else {
                        header_bytes.push(0 as u8);
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
                    variable_bytes.extend_from_slice(&b);
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

#[derive(Clone, Copy, Debug)]
#[repr(u8)]
pub enum DataType {
    Int,
    VarChar,
    Float,
    Boolean,
    Blob,
}
impl DataType {
    fn from_u8(byte: u8) -> Self {
        match byte {
            0 => Self::Int,
            1 => Self::VarChar,
            2 => Self::Float,
            3 => Self::Boolean,
            4 => Self::Blob,
            _ => unreachable!(),
        }
    }
    fn size(&self) -> usize {
        match self {
            Self::Int |Self::Float => 4,
            Self::Boolean => 1,
            Self::VarChar | Self::Blob => 4,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Value {
    Int(i32),
    VarChar(String),
    Float(f32),
    Boolean(bool),
    Blob(Vec<u8>),
    Null,
}
impl Value {
    pub fn size(&self) -> usize {
        match self {
            Self::Int(i) => size_of_val(i),
            Self::Float(f) => size_of_val(f),
            Self::VarChar(s) => s.len(),
            Self::Boolean(_) => 1,
            Self::Blob(b) => b.len(),
            Self::Null => 0,
        }
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
    pub fn to_be_bytes(&self) -> Result<Vec<u8>> {
        let mut bytes = vec![];
        let name_len = u8::try_from(self.name.len())?;
        bytes.extend_from_slice(&name_len.to_be_bytes());
        bytes.extend_from_slice(self.name.as_bytes());
        bytes.push(self.data_type as u8);
        bytes.push(u8::from(self.is_key));
        bytes.push(u8::from(self.is_nullable));
        Ok(bytes)
    }

    pub fn from_be_bytes(bytes: &[u8]) -> Result<(Self, usize)> {
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
    pub fn to_be_bytes(&self) -> Result<Vec<u8>> {
        let mut bytes = vec![];

        let num_attrs = u32::try_from(self.attributes.len())?;
        bytes.extend_from_slice(&num_attrs.to_be_bytes());
        for attr in &self.attributes {
            let attr_bytes = attr.to_be_bytes()?;
            bytes.extend_from_slice(&attr_bytes);
        }
        Ok(bytes)
    }
    pub fn from_be_bytes(bytes: &[u8]) -> Result<Self> {
        let mut data = bytes;

        let num_attrs = u32::from_be_bytes(
            bytes[0..4]
                .try_into()
                .context("Failed to get u32 num_attrs.")?,
        );
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

#[allow(dead_code)]
pub struct Table {
    name: String,
    file_id: u32,
    current_fsm_idx: AtomicU64,
    schema: TableSchema,
    buffer_manager: Arc<BufferPool>,
    catalog_manager: Arc<RwLock<CatalogManager>>,
    last_known_free_page: AtomicU64,
}
impl Table {
    pub fn new(
        name: &str,
        schema: &TableSchema,
        bp: Arc<BufferPool>,
        catalog: Arc<RwLock<CatalogManager>>,
    ) -> anyhow::Result<Self> {
        let cat = catalog.clone();
        let bp = bp.clone();
        // 1. Register with catalog, get file_id.
        let file_id = cat.write().create_table(name, schema)?;

        // 2. Initialize the page file.
        // 2.1 Catalog file initialization (write schema).
        let mut page = bp.get_page(PageId {
            file_id,
            page_num: 0,
        })?;
        let mut page = page.as_catalog_mut()?;
        tracing::debug!("{:?}", page.header().num_entries());
        if page.header().num_entries() == 0 {
            page.insert(&schema.to_be_bytes()?)?;
            // 2.2 Write free space map page.
            {
                let mut page = bp.create_page(file_id, PageKind::FreeSpaceMap)?;
                let mut page = page.as_fsm_mut()?;
                page.set_fsm_num(0);
                assert!(page.header().page_id() == 1);
            }
            // 2.3 Make the first heap page for storage
            {
                let mut page = bp.create_page(file_id, PageKind::Heap)?;
                //assert!(page.handle.page_id.page_num == 2);
                let page = page.as_heap_mut()?;
                assert!(page.header().page_id() == 2);
            }
        }

        Ok(Self {
            name: name.to_string(),
            file_id,
            current_fsm_idx: AtomicU64::new(FIRST_FSM_PAGE),
            schema: schema.clone(),
            buffer_manager: bp.clone(),
            catalog_manager: catalog.clone(),
            last_known_free_page: AtomicU64::new(2),
        })
    }

    fn find_page_with_free_space(&self) -> anyhow::Result<u64> {
        let fsm = self.buffer_manager.get_page(PageId {
            file_id: self.file_id,
            page_num: self.current_fsm_idx.load(Ordering::Relaxed),
        })?;
        let fsm = fsm.as_fsm()?;
        let ffp = fsm.find_first_free_page(self.last_known_free_page.load(Ordering::Relaxed));
        self.last_known_free_page.store(ffp, Ordering::Relaxed);
        Ok(ffp)
    }

    // TODO: record is just a byte slice atm. replace with something more
    // sophisticated (RecordBuilder based on schema?)
    pub fn insert_record(&self, record: &[u8]) -> anyhow::Result<RecordId> {
        tracing::debug!("Inserting record: {:?}", record);
        let mut free_page = self.find_page_with_free_space()?;
        tracing::debug!("Inserting to page: {:?}", free_page);
        if free_page == u64::MAX {
            let new_page = self
                .buffer_manager
                .create_page(self.file_id, PageKind::Heap)?;
            free_page = new_page.handle.page_id.page_num;
        }

        let mut page = self.buffer_manager.get_page(PageId {
            file_id: self.file_id,
            page_num: free_page,
        })?;
        let mut heap_page = page.as_heap_mut()?;
        match heap_page.insert(record) {
            Ok(slot_num) => Ok(RecordId {
                page: free_page,
                slot: slot_num.slot,
            }),
            Err(_) => {
                let mut fsm = self.buffer_manager.get_page(PageId {
                    file_id: self.file_id,
                    page_num: self.current_fsm_idx.load(Ordering::Relaxed),
                })?;
                let mut fsm_page = fsm.as_fsm_mut()?;
                fsm_page.set_page_full(free_page);

                if fsm_page.is_full() {
                    let mut new_fsm_page = self
                        .buffer_manager
                        .create_page(self.file_id, PageKind::FreeSpaceMap)?;
                    let new_fsm = new_fsm_page.as_fsm_mut()?;
                    self.current_fsm_idx
                        .store(new_fsm.header().page_id(), Ordering::Relaxed);
                    fsm_page
                        .header_mut()
                        .set_next_page(self.current_fsm_idx.load(Ordering::Relaxed));
                }

                let mut new_heap_frame = self
                    .buffer_manager
                    .create_page(self.file_id, PageKind::Heap)?;
                let mut new_heap_page = new_heap_frame.as_heap_mut()?;
                let slot = new_heap_page.insert(record)?;
                Ok(RecordId {
                    page: new_heap_page.header().page_id(),
                    slot: slot.slot,
                })
            }
        }
    }
}
