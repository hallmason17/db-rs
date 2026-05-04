use std::sync::Arc;

use crate::{buffer_mgr::BufferPool, storage_mgr::StorageManager};

pub struct RecordId {
    page: u32,
    slot: u32,
}

pub struct Record {
    rid: RecordId,
    data: Vec<u8>,
}

#[derive(Clone, Copy)]
#[repr(u8)]
pub enum DataType {
    Int,
    String,
    Float,
    Boolean,
    Null,
}

pub enum Value {
    Int(i32),
    String(String),
    Float(f32),
    Boolean(bool),
    Null,
}

#[repr(C)]
pub struct ColumnDefinition {
    name: String,
    data_type: DataType,
    is_key: bool,
}
impl ColumnDefinition {
    pub fn as_bytes(&self) -> Vec<u8> {
        let mut bytes = vec![];
        let name_len = self.name.len();
        bytes.extend_from_slice(&name_len.to_be_bytes());
        bytes.extend_from_slice(self.name.as_bytes());
        bytes.push(self.data_type as u8);
        bytes.push(self.is_key as u8);
        bytes
    }

    pub fn from_bytes(bytes: &[u8]) -> Self {
        Self {
            name: String::new(),
            data_type: DataType::Null,
            is_key: false,
        }
    }
}

pub struct TableSchema {
    attributes: Vec<ColumnDefinition>,
}
impl TableSchema {
    pub fn new(attributes: Vec<ColumnDefinition>) -> Self {
        Self { attributes }
    }
    pub fn as_bytes(&self) -> Vec<u8> {
        let mut bytes = vec![];

        let num_attrs = self.attributes.len() as u32;
        bytes.extend_from_slice(&num_attrs.to_be_bytes());
        for attr in self.attributes.iter() {
            let attr_bytes = attr.as_bytes();
            bytes.extend_from_slice(&attr_bytes);
        }
        bytes
    }
    pub fn from_bytes(bytes: &[u8]) -> Self {
        let mut data = &bytes[..];

        let num_attrs = u32::from_be_bytes(bytes[..4].try_into().unwrap());
        data = &data[4..];
        let mut attrs = Vec::new();
        for _ in 0..num_attrs {
            let attr = ColumnDefinition::from_bytes(data);
            attrs.push(attr);
        }
        Self { attributes: attrs }
    }
}

pub struct Table {
    name: String,
    schema: TableSchema,
    buffer_manager: Arc<BufferPool>,
    storage_manager: Arc<StorageManager>,
}
