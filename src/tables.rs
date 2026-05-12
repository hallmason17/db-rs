use anyhow::{Context, Result};
use parking_lot::RwLock;
use std::sync::Arc;

use crate::{buffer_pool::BufferPool, storage::StorageManager, DbError, DbInputError, DbResult};

#[allow(dead_code)]
pub struct RecordId {
    page: u32,
    slot: u32,
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

#[allow(dead_code)]
pub struct Table {
    name: String,
    schema: TableSchema,
    buffer_manager: Arc<RwLock<BufferPool>>,
    storage_manager: Arc<RwLock<StorageManager>>,
}
