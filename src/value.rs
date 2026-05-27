use std::cmp::Ordering;

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
    pub fn from_u8(byte: u8) -> Self {
        match byte {
            0 => Self::Int,
            1 => Self::VarChar,
            2 => Self::Float,
            3 => Self::Boolean,
            4 => Self::Blob,
            _ => unreachable!(),
        }
    }
    pub fn size(&self) -> usize {
        match self {
            Self::Int | Self::Float => 4,
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

    pub fn datatype(&self) -> Option<DataType> {
        match self {
            Self::Blob(_) => Some(DataType::Blob),
            Self::Boolean(_) => Some(DataType::Boolean),
            Self::Int(_) => Some(DataType::Int),
            Self::Float(_) => Some(DataType::Float),
            Self::VarChar(_) => Some(DataType::VarChar),
            Self::Null => None,
        }
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Blob(l0), Self::Blob(r0)) => l0 == r0,
            (Self::Boolean(l0), Self::Boolean(r0)) => l0 == r0,
            (Self::Int(l0), Self::Int(r0)) => l0 == r0,
            (Self::Float(l0), Self::Float(r0)) => l0 == r0,
            (Self::Float(l0), Self::Int(r0)) => *l0 == *r0 as f32,
            (Self::Int(l0), Self::Float(r0)) => *l0 as f32 == *r0,
            (Self::VarChar(l0), Self::VarChar(r0)) => l0 == r0,
            (Self::Null, Self::Null) => true,
            _ => false,
        }
    }
}
impl Eq for Value {}

impl PartialOrd for Value {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Value {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match (self, other) {
            (Self::Blob(l0), Self::Blob(r0)) => l0.cmp(r0),
            (Self::Boolean(l0), Self::Boolean(r0)) => l0.cmp(r0),
            (Self::Int(l0), Self::Int(r0)) => l0.cmp(r0),
            (Self::Float(l0), Self::Float(r0)) => l0.total_cmp(r0),
            (Self::Int(l0), Self::Float(r0)) => (*l0 as f32).total_cmp(r0),
            (Self::Float(l0), Self::Int(r0)) => l0.total_cmp(&(*r0 as f32)),
            (Self::VarChar(l0), Self::VarChar(r0)) => l0.cmp(r0),
            (Self::Null, Self::Null) => Ordering::Equal,

            (Self::Null, _) => Ordering::Less,
            (_, Self::Null) => Ordering::Greater,
            (Self::Boolean(_), _) => Ordering::Less,
            (_, Self::Boolean(_)) => Ordering::Greater,
            (Self::Float(_), _) => Ordering::Less,
            (_, Self::Float(_)) => Ordering::Greater,
            (Self::Int(_), _) => Ordering::Less,
            (_, Self::Int(_)) => Ordering::Greater,
            (Self::VarChar(_), _) => Ordering::Less,
            (_, Self::VarChar(_)) => Ordering::Greater,
        }
    }
}
