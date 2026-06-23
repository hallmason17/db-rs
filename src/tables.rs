/* Copyright (C) 2026 Mason Hall.
 *
 * GNU General Public License v3.0+ (see COPYING or https://www.gnu.org/licenses/gpl-3.0.txt)
 */
use std::borrow::Cow;
use std::sync::Arc;

use crate::value::ValueRef;
use crate::{
    PageId,
    buffer_pool::BufferPool,
    error::{Error, InputError, Result},
    ids::{FileId, TableId},
    page::{PageAccessor, PageHeaderReader, PageKind, SlottedPageMut},
    value::{DataType, Value},
};

#[derive(Debug)]
pub struct RecordId {
    pub page: u64,
    pub slot: u16,
}

#[allow(dead_code)]
pub struct Record {
    pub rid: RecordId,
    pub data: Vec<u8>,
}

pub struct TupleRef<'a> {
    bytes: &'a [u8],
    schema: &'a TableSchema,
}
impl<'a> TupleRef<'a> {
    pub fn new(bytes: &'a [u8], schema: &'a TableSchema) -> TupleRef<'a> {
        TupleRef { bytes, schema }
    }
    pub fn bytes(&self) -> &[u8] {
        self.bytes
    }

    pub fn to_owned(self) -> Result<Tuple> {
        Tuple::deserialize(self.bytes, self.schema)
    }

    pub fn get_attr(&self, idx: usize) -> Result<ValueRef<'a>> {
        let mut pos = self.schema.attributes.len().div_ceil(8);
        for (i, col) in self.schema.attributes.iter().enumerate() {
            match col.data_type {
                DataType::Boolean => {
                    if i == idx {
                        return Ok(ValueRef::Boolean(self.bytes[pos].try_into()?));
                    }
                    pos += 1;
                }
                DataType::Float => {
                    if i == idx {
                        return Ok(ValueRef::Float(f32::from_be_bytes(
                            self.bytes[pos..pos + 4].try_into()?,
                        )));
                    }
                    pos += 4;
                }
                DataType::Int => {
                    if i == idx {
                        return Ok(ValueRef::Int(i32::from_be_bytes(
                            self.bytes[pos..pos + 4].try_into()?,
                        )));
                    }
                    pos += 4;
                }
                DataType::VarChar => {
                    if i == idx {
                        let offset =
                            u16::from_be_bytes(self.bytes[pos..pos + 2].try_into()?) as usize;
                        pos += 2;
                        let size =
                            u16::from_be_bytes(self.bytes[pos..pos + 2].try_into()?) as usize;
                        return Ok(ValueRef::VarChar(Cow::Borrowed(str::from_utf8(
                            &self.bytes[offset..offset + size],
                        )?)));
                    }
                    pos += 4;
                }
                DataType::Blob => {
                    if i == idx {
                        let offset =
                            u16::from_be_bytes(self.bytes[pos..pos + 2].try_into()?) as usize;
                        pos += 2;
                        let size =
                            u16::from_be_bytes(self.bytes[pos..pos + 2].try_into()?) as usize;
                        return Ok(ValueRef::Blob(Cow::Borrowed(
                            &self.bytes[offset..offset + size],
                        )));
                    }
                    pos += 4;
                }
            }
        }
        Err(Error::Unknown)
    }
}
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Tuple {
    pub values: Vec<Value>,
}
impl Tuple {
    pub fn new(values: Vec<Value>) -> Self {
        Self { values }
    }

    pub fn header_size(&self) -> usize {
        let mut size = 0;

        // Null bitmap size
        size += self.values.len().div_ceil(8);
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
        let header_size = self.header_size();
        let mut header_bytes = Vec::with_capacity(header_size);
        let mut variable_bytes = Vec::new();

        let mut variable_byte_offset = header_size;

        let null_bitmap = self.gen_null_bitmap();

        header_bytes.extend_from_slice(&null_bitmap);
        header_bytes.resize(self.values.len().div_ceil(8), 0);

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
                    header_bytes.resize(header_bytes.len() + col.data_type.size(), 0u8);
                }
            }
        }

        header_bytes.extend(&variable_bytes);
        header_bytes
    }

    pub fn deserialize(bytes: &[u8], schema: &TableSchema) -> Result<Self> {
        let mut values = Vec::with_capacity(schema.attributes.len());
        let bitmap_size = schema.attributes.len().div_ceil(8);
        let bitmap = &bytes[..bitmap_size];
        let is_value_null = |bitmap: &[u8], idx: usize| {
            let byte = idx / 8;
            (bitmap[byte] >> (7 - idx % 8)) & 1 == 1
        };
        let mut pos = bitmap_size;
        for (i, attr) in schema.attributes.iter().enumerate() {
            if is_value_null(bitmap, i) {
                values.push(Value::Null);
                pos += attr.data_type.size();
                continue;
            }
            match attr.data_type {
                DataType::Int => {
                    values.push(Value::Int(i32::from_be_bytes(
                        bytes[pos..pos + 4].try_into()?,
                    )));
                    pos += 4;
                }
                DataType::Float => {
                    values.push(Value::Float(f32::from_be_bytes(
                        bytes[pos..pos + 4].try_into()?,
                    )));
                    pos += 4;
                }
                DataType::Boolean => {
                    values.push(Value::Boolean(bytes[pos] == 1));
                    pos += 1;
                }
                DataType::VarChar => {
                    let offset = u16::from_be_bytes(bytes[pos..pos + 2].try_into()?) as usize;
                    pos += 2;
                    let size = u16::from_be_bytes(bytes[pos..pos + 2].try_into()?) as usize;
                    pos += 2;
                    values.push(Value::VarChar(
                        String::from_utf8(bytes[offset..offset + size].to_vec())?.into(),
                    ));
                }
                DataType::Blob => {
                    let offset = u16::from_be_bytes(bytes[pos..pos + 2].try_into()?) as usize;
                    pos += 2;
                    let size = u16::from_be_bytes(bytes[pos..pos + 2].try_into()?) as usize;
                    pos += 2;
                    values.push(Value::Blob(Arc::from(&bytes[offset..offset + size])));
                }
            }
        }
        Ok(Self { values })
    }

    pub fn from_record(record: &Record, schema: &TableSchema) -> Result<Self> {
        Self::deserialize(&record.data, schema)
    }
}

#[derive(Debug, Clone)]
#[repr(C)]
pub struct ColumnDefinition {
    pub name: String,
    pub data_type: DataType,
    pub is_nullable: bool,
}
impl ColumnDefinition {
    pub fn new(name: String, data_type: DataType, is_nullable: bool) -> Result<Self> {
        if name.len() > u8::MAX as usize {
            return Err(Error::InputError(InputError::StringTooLong));
        }
        Ok(Self {
            name,
            data_type,
            is_nullable,
        })
    }
    pub fn to_be_bytes(&self) -> Result<Vec<u8>> {
        let mut bytes = vec![];
        let name_len = u8::try_from(self.name.len())?;
        bytes.extend_from_slice(&name_len.to_be_bytes());
        bytes.extend_from_slice(self.name.as_bytes());
        bytes.push(self.data_type as u8);
        // bytes.push(u8::from(self.is_key));
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
        // let is_key = match data[0] {
        //     0 => false,
        //     1 => true,
        //     _ => {
        //         eprintln!("Casting {} to a bool?", data[0]);
        //         unreachable!()
        //     }
        // };
        // data = &data[1..];
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

        let num_attrs = u32::from_be_bytes(bytes[0..4].try_into()?);
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

    pub fn is_valid_tuple(&self, tuple: &Tuple) -> bool {
        if self.attributes.len() != tuple.values.len() {
            return false;
        }
        for (attr, val) in self.attributes.iter().zip(&tuple.values) {
            if val.datatype().unwrap() != attr.data_type {
                return false;
            }
            if val.datatype().is_none() && !attr.is_nullable {
                return false;
            }
        }
        true
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct Table {
    pub table_id: TableId,
    pub file_id: FileId,
    pub name: String,
    pub schema: TableSchema,
    pub(crate) current_heap_page: u64,
}
impl Table {
    pub fn new(
        name: &str,
        schema: &TableSchema,
        bp: &mut BufferPool,
        file_id: FileId,
    ) -> Result<Self> {
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
                if page.header().page_id() != 0 {
                    return Err(Error::CorruptPageFile);
                }
                page.insert(&schema.to_be_bytes()?)?;
            }
            // 2.3 Make the first heap page for storage
            {
                let mut page = bp.create_page(file_id, PageKind::Heap)?;
                let page = page.as_heap_mut()?;
                if page.header().page_id() != 1 {
                    return Err(Error::CorruptPageFile);
                }
            }
        }

        Ok(Self {
            name: name.to_string(),
            table_id: TableId(file_id.0),
            file_id,
            schema: schema.clone(),
            current_heap_page: 1,
        })
    }

    pub fn open(table_id: TableId, file_id: FileId, name: &str, schema: &TableSchema) -> Self {
        Self {
            table_id,
            file_id,
            name: name.to_string(),
            schema: schema.clone(),
            current_heap_page: 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_attr_works() {
        let attrs = vec![
            ColumnDefinition::new("id".into(), DataType::Int, true, false).unwrap(),
            ColumnDefinition::new("name".into(), DataType::VarChar, false, true).unwrap(),
            ColumnDefinition::new("score".into(), DataType::Float, false, false).unwrap(),
            ColumnDefinition::new("active".into(), DataType::Boolean, false, false).unwrap(),
        ];
        let schema = TableSchema::new(&attrs);
        let tuple = Tuple::new(vec![
            Value::Int(1),
            Value::VarChar("hello".into()),
            Value::Float(5.0),
            Value::Boolean(true),
        ]);
        let bytes = tuple.serialize(&schema);
        let tuple_ref = TupleRef::new(&bytes, &schema);
        let attr = tuple_ref.get_attr(0).unwrap();
        let attr1 = tuple_ref.get_attr(1).unwrap();
        let attr2 = tuple_ref.get_attr(2).unwrap();
        let attr3 = tuple_ref.get_attr(3).unwrap();
        assert_eq!(attr, ValueRef::Int(1));
        assert_eq!(attr1, ValueRef::VarChar(Cow::Borrowed("hello")));
        assert_eq!(attr2, ValueRef::Float(5.0));
        assert_eq!(attr3, ValueRef::Boolean(true));
    }

    #[test]
    fn column_definition_roundtrip() {
        let col = ColumnDefinition::new("id".into(), DataType::Int, true, false).unwrap();
        let bytes = col.to_be_bytes().unwrap();
        let (decoded, _) = ColumnDefinition::from_be_bytes(&bytes).unwrap();
        assert_eq!(col.name, decoded.name);
        assert_eq!(col.data_type as u8, decoded.data_type as u8);
        assert_eq!(col.is_key, decoded.is_key);
        assert_eq!(col.is_nullable, decoded.is_nullable);
    }

    #[test]
    fn table_schema_roundtrip() {
        let attrs = vec![
            ColumnDefinition::new("id".into(), DataType::Int, true, false).unwrap(),
            ColumnDefinition::new("name".into(), DataType::VarChar, false, true).unwrap(),
            ColumnDefinition::new("score".into(), DataType::Float, false, false).unwrap(),
            ColumnDefinition::new("active".into(), DataType::Boolean, false, false).unwrap(),
        ];
        let schema = TableSchema::new(&attrs);
        let bytes = schema.to_be_bytes().unwrap();
        let decoded = TableSchema::from_be_bytes(&bytes).unwrap();
        assert_eq!(decoded.attributes.len(), 4);
        for (a, b) in schema.attributes.iter().zip(decoded.attributes.iter()) {
            assert_eq!(a.name, b.name);
            assert_eq!(a.data_type as u8, b.data_type as u8);
        }
    }

    #[test]
    fn tuple_serialize_int() {
        let col = ColumnDefinition::new("val".into(), DataType::Int, false, false).unwrap();
        let schema = TableSchema::new(&[col]);
        let tuple = Tuple::new(vec![Value::Int(42)]);
        let bytes = tuple.serialize(&schema);
        // null bitmap (1 byte) + int (4 bytes) = 5 bytes
        assert_eq!(bytes.len(), 5);
        assert_eq!(bytes[0], 0); // no nulls
        assert_eq!(&bytes[1..5], &42i32.to_be_bytes());
    }

    #[test]
    fn tuple_serialize_varchar() {
        let col = ColumnDefinition::new("name".into(), DataType::VarChar, false, true).unwrap();
        let schema = TableSchema::new(&[col]);
        let tuple = Tuple::new(vec![Value::VarChar("hello".into())]);
        let bytes = tuple.serialize(&schema);
        // null bitmap (1) + offset (2) + len (2) + data (5) = 10 bytes
        assert_eq!(bytes.len(), 10);
        assert_eq!(&bytes[5..10], b"hello");
    }

    #[test]
    fn tuple_serialize_mixed() {
        let attrs = vec![
            ColumnDefinition::new("id".into(), DataType::Int, true, false).unwrap(),
            ColumnDefinition::new("name".into(), DataType::VarChar, false, true).unwrap(),
            ColumnDefinition::new("score".into(), DataType::Float, false, false).unwrap(),
            ColumnDefinition::new("active".into(), DataType::Boolean, false, false).unwrap(),
        ];
        let schema = TableSchema::new(&attrs);
        let tuple = Tuple::new(vec![
            Value::Int(1),
            Value::VarChar("hello".into()),
            Value::Float(5.0),
            Value::Boolean(true),
        ]);
        let bytes = tuple.serialize(&schema);
        let header_size = tuple.header_size() as u16;

        assert_eq!(bytes.len(), header_size as usize + "hello".len());
        assert_eq!(bytes[0], 0b0000_0000);
        assert_eq!(&bytes[1..5], &1i32.to_be_bytes());
        assert_eq!(&bytes[5..7], header_size.to_be_bytes());
        assert_eq!(&bytes[7..9], ("hello".len() as u16).to_be_bytes());
        assert_eq!(&bytes[9..13], &5.0f32.to_be_bytes());
        assert_eq!(&bytes[14..19], "hello".as_bytes());
    }

    #[test]
    fn tuple_deserialize_int() {
        let col = ColumnDefinition::new("val".into(), DataType::Int, false, false).unwrap();
        let schema = TableSchema::new(&[col]);
        let tuple = Tuple::new(vec![Value::Int(42)]);
        let bytes = tuple.serialize(&schema);
        let des_tuple = Tuple::deserialize(&bytes, &schema).unwrap();
        assert_eq!(tuple, des_tuple);
    }

    #[test]
    fn tuple_deserialize_varchar() {
        let col = ColumnDefinition::new("name".into(), DataType::VarChar, false, true).unwrap();
        let schema = TableSchema::new(&[col]);
        let tuple = Tuple::new(vec![Value::VarChar("hello".into())]);
        let bytes = tuple.serialize(&schema);
        let des_tuple = Tuple::deserialize(&bytes, &schema).unwrap();
        assert_eq!(tuple, des_tuple);
    }

    #[test]
    fn tuple_deserialize_mixed() {
        let attrs = vec![
            ColumnDefinition::new("id".into(), DataType::Int, true, false).unwrap(),
            ColumnDefinition::new("name".into(), DataType::VarChar, false, true).unwrap(),
            ColumnDefinition::new("score".into(), DataType::Float, false, false).unwrap(),
            ColumnDefinition::new("active".into(), DataType::Boolean, false, false).unwrap(),
        ];
        let schema = TableSchema::new(&attrs);
        let tuple = Tuple::new(vec![
            Value::Int(1),
            Value::VarChar("hello".into()),
            Value::Float(5.0),
            Value::Boolean(true),
        ]);
        let bytes = tuple.serialize(&schema);
        let des_tuple = Tuple::deserialize(&bytes, &schema).unwrap();
        assert_eq!(tuple, des_tuple);
    }

    #[test]
    fn tuple_serialize_null_bitmap() {
        let cols = vec![
            ColumnDefinition::new("a".into(), DataType::Int, false, true).unwrap(),
            ColumnDefinition::new("b".into(), DataType::Int, false, true).unwrap(),
            ColumnDefinition::new("c".into(), DataType::Int, false, true).unwrap(),
        ];
        let schema = TableSchema::new(&cols);
        // Second value is null
        let tuple = Tuple::new(vec![Value::Int(10), Value::Null, Value::Int(30)]);
        let bytes = tuple.serialize(&schema);
        // null bitmap (1 byte): bit 1 (0-indexed from MSB) should be set
        assert_eq!(bytes[0], 0b0100_0000);
        assert_eq!(&bytes[1..5], &10i32.to_be_bytes());
        assert_eq!(&bytes[9..13], &30i32.to_be_bytes());
    }

    #[test]
    fn tuple_serialize_mixed_types() {
        let cols = vec![
            ColumnDefinition::new("id".into(), DataType::Int, true, false).unwrap(),
            ColumnDefinition::new("name".into(), DataType::VarChar, false, true).unwrap(),
            ColumnDefinition::new("gpa".into(), DataType::Float, false, false).unwrap(),
        ];
        let schema = TableSchema::new(&cols);
        let tuple = Tuple::new(vec![
            Value::Int(7),
            Value::VarChar("bob".into()),
            Value::Float(3.5),
        ]);
        let bytes = tuple.serialize(&schema);

        // header: null_map(1) + int(4) + vc_offset(2) + vc_len(2) + float(4) = 13
        // variable: "bob"(3) = 3; total = 16
        assert_eq!(bytes.len(), 16);
        assert_eq!(bytes[0], 0);
        assert_eq!(&bytes[1..5], &7i32.to_be_bytes());
        assert_eq!(&bytes[9..13], &3.5f32.to_be_bytes());
        assert_eq!(&bytes[13..16], b"bob");
    }
}
