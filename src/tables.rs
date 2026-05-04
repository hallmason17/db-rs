pub struct RecordId {
    page: u64,
    slot: u32,
}

pub struct Record {
    rid: RecordId,
    data: Vec<u8>,
}

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

pub struct ColumnDefinition {
    name: String,
    data_type: DataType,
    is_key: bool,
}

pub struct TableSchema {
    name: String,
    attributes: Vec<ColumnDefinition>,
}
