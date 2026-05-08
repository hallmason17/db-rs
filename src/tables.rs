use parking_lot::RwLock;
use std::sync::Arc;

use crate::{DbError, DbInputError, DbResult, buffer_pool::BufferPool, storage::StorageManager};

pub struct RecordId {
    page: u32,
    slot: u32,
}

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
            4 => Self::Null,
            _ => Self::Invalid,
        }
    }
}

pub enum Value {
    Int(i32),
    String(String),
    Float(f32),
    Boolean(bool),
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
    pub fn as_bytes(&self) -> Vec<u8> {
        let mut bytes = vec![];
        let name_len = self.name.len() as u8;
        bytes.extend_from_slice(&name_len.to_be_bytes());
        bytes.extend_from_slice(self.name.as_bytes());
        bytes.push(self.data_type as u8);
        bytes.push(self.is_key as u8);
        bytes
    }

    pub fn from_bytes(bytes: &[u8]) -> anyhow::Result<Self> {
        let mut data = &bytes[..];
        let name_len = u8::from_be_bytes(data[..0].try_into()?);
        data = &data[0..];
        let name = String::from_utf8_lossy(&data[..name_len as usize]);
        data = &data[name_len as usize..];
        let data_type = DataType::from_u8(*data.first().unwrap());
        data = &data[0..];
        let is_key = bool::try_from(*data.first().unwrap())?;
        Ok(Self {
            name: name.to_string(),
            data_type,
            is_key,
        })
    }
}

#[derive(Debug, Clone)]
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
        tracing::debug!("Schema bytes: {:?}", bytes);
        bytes
    }
    pub fn from_bytes(bytes: &[u8]) -> anyhow::Result<Self> {
        let mut data = &bytes[..];

        let num_attrs = u32::from_be_bytes(bytes[..4].try_into().unwrap());
        data = &data[4..];
        let mut attrs = Vec::new();
        for _ in 0..num_attrs {
            let attr = ColumnDefinition::from_bytes(data)?;
            attrs.push(attr);
        }
        Ok(Self { attributes: attrs })
    }
}

pub struct Table {
    name: String,
    schema: TableSchema,
    buffer_manager: Arc<RwLock<BufferPool>>,
    storage_manager: Arc<RwLock<StorageManager>>,
}
