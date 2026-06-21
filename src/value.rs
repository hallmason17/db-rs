/* Copyright (C) 2026 Mason Hall.
 *
 * GNU General Public License v3.0+ (see COPYING or https://www.gnu.org/licenses/gpl-3.0.txt)
 */
use std::borrow::Cow;
use std::cmp::Ordering;
use std::sync::Arc;

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

#[derive(Debug, Clone, PartialEq)]
pub enum ValueRef<'a> {
    Int(i32),
    VarChar(Cow<'a, str>),
    Float(f32),
    Boolean(bool),
    Blob(Cow<'a, [u8]>),
    Null,
}

impl<'a> ValueRef<'a> {
    pub(crate) fn from_owned(v: &'a Value) -> ValueRef<'a> {
        match v {
            Value::Int(i) => ValueRef::Int(*i),
            Value::VarChar(c) => ValueRef::VarChar(Cow::Borrowed(c)),
            Value::Float(f) => ValueRef::Float(*f),
            Value::Boolean(b) => ValueRef::Boolean(*b),
            Value::Blob(b) => ValueRef::Blob(Cow::Borrowed(b)),
            Value::Null => ValueRef::Null,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Value {
    Int(i32),
    VarChar(Arc<str>),
    Float(f32),
    Boolean(bool),
    Blob(Arc<[u8]>),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cross_type_equality_int_float() {
        assert_eq!(Value::Int(42), Value::Float(42.0));
        assert_eq!(Value::Float(3.0), Value::Int(3));
        assert_ne!(Value::Float(3.14), Value::Int(3));
        assert_ne!(Value::Int(1), Value::VarChar("1".into()));
        assert_ne!(Value::Null, Value::Int(0));
    }

    #[test]
    fn null_equality_only_with_itself() {
        assert_eq!(Value::Null, Value::Null);
        assert_ne!(Value::Null, Value::Boolean(false));
        assert_ne!(Value::Int(0), Value::Null);
    }

    #[test]
    fn cross_type_ordering() {
        assert!(Value::Null < Value::Int(0));
        assert!(Value::Null < Value::Float(-1.0));
        assert!(Value::Boolean(true) < Value::Int(0));
        assert!(Value::Int(0) < Value::VarChar("".into()));
        assert!(Value::Float(std::f32::NEG_INFINITY) > Value::Null);
        assert!(Value::VarChar("a".into()) > Value::Int(9999));
        assert!(Value::Blob(Arc::new([0])) > Value::Int(9999));
    }

    #[test]
    fn same_type_ordering_within_type() {
        assert!(Value::Int(1) < Value::Int(2));
        assert!(Value::Float(1.5) > Value::Float(1.0));
        assert!(Value::VarChar("a".into()) < Value::VarChar("b".into()));
        assert!(Value::Boolean(false) == Value::Boolean(false));
        assert!(Value::Blob(Arc::new([1, 2])) < Value::Blob(Arc::new([1, 2, 3])));
    }

    #[test]
    fn int_float_cross_type_ordering() {
        assert_eq!(Value::Int(5).cmp(&Value::Float(5.0)), Ordering::Equal);
        assert!(Value::Int(5) < Value::Float(5.5));
        assert!(Value::Float(4.9) < Value::Int(5));
    }
}
