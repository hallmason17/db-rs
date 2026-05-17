use anyhow::{Context, Result};
use parking_lot::RwLock;
use std::sync::Arc;

use crate::{
    DbError, DbInputError, DbResult, PageId,
    buffer_pool::BufferPool,
    catalog::CatalogManager,
    page::{
        PageAccessor, PageAccessorMut, PageHeaderReader, PageKind, SlottedPageMut,
        fsm::{FreeSpaceMapper, FreeSpaceMapperMut},
    },
};

#[allow(dead_code)]
pub struct RecordId {
    page: u64,
    slot: u16,
}

#[allow(dead_code)]
pub struct Record {
    rid: RecordId,
    data: Vec<u8>,
}

#[derive(Clone, Copy, Debug)]
#[repr(u8)]
pub enum DataType {
    Int,
    String,
    Float,
    Boolean,
    Blob,
    Null,
    Invalid,
}
impl DataType {
    fn from_u8(byte: u8) -> Self {
        match byte {
            0 => Self::Int,
            1 => Self::String,
            2 => Self::Float,
            3 => Self::Boolean,
            4 => Self::Blob,
            5 => Self::Null,
            _ => Self::Invalid,
        }
    }
}

pub enum Value {
    Int(i32),
    VarChar(String),
    Float(f32),
    Boolean(bool),
    Blob(Vec<u8>),
    Null,
}

#[derive(Debug, Clone)]
#[repr(C)]
pub struct ColumnDefinition {
    pub name: String,
    pub data_type: DataType,
    pub is_key: bool,
}
impl ColumnDefinition {
    pub fn new(name: String, data_type: DataType, is_key: bool) -> DbResult<Self> {
        if name.len() > u8::MAX as usize {
            return Err(DbError::InputError(DbInputError::StringTooLong));
        }
        Ok(Self {
            name,
            data_type,
            is_key,
        })
    }
    pub fn to_be_bytes(&self) -> Result<Vec<u8>> {
        let mut bytes = vec![];
        let name_len = u8::try_from(self.name.len())?;
        bytes.extend_from_slice(&name_len.to_be_bytes());
        bytes.extend_from_slice(self.name.as_bytes());
        bytes.push(self.data_type as u8);
        bytes.push(u8::from(self.is_key));
        Ok(bytes)
    }

    pub fn from_be_bytes(bytes: &[u8]) -> Result<(Self, usize)> {
        let mut data = bytes;
        let mut len = 3;
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
        Ok((
            Self {
                name: name.to_string(),
                data_type,
                is_key,
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
    current_fsm_idx: u64,
    schema: TableSchema,
    buffer_manager: Arc<BufferPool>,
    catalog_manager: Arc<RwLock<CatalogManager>>,
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
                let page = page.as_fsm_mut()?;
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
            current_fsm_idx: FIRST_FSM_PAGE,
            schema: schema.clone(),
            buffer_manager: bp.clone(),
            catalog_manager: catalog.clone(),
        })
    }

    fn find_page_with_free_space(&self) -> anyhow::Result<u64> {
        let fsm = self.buffer_manager.get_page(PageId {
            file_id: self.file_id,
            page_num: self.current_fsm_idx,
        })?;
        let fsm = fsm.as_fsm()?;
        Ok(fsm.find_first_free_page())
    }

    // TODO: record is just a byte slice atm. replace with something more
    // sophisticated (RecordBuilder based on schema?)
    pub fn insert_record(&mut self, record: &[u8]) -> anyhow::Result<RecordId> {
        let mut free_page = self.find_page_with_free_space()?;
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
                    page_num: self.current_fsm_idx,
                })?;
                let mut fsm_page = fsm.as_fsm_mut()?;
                fsm_page.set_page_full(free_page);

                if fsm_page.is_full() {
                    let mut new_fsm_page = self
                        .buffer_manager
                        .create_page(self.file_id, PageKind::FreeSpaceMap)?;
                    let new_fsm = new_fsm_page.as_fsm_mut()?;
                    self.current_fsm_idx = new_fsm.header().page_id();
                    fsm_page.header_mut().set_next_page(self.current_fsm_idx);
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
