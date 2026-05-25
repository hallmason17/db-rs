use std::{array::TryFromSliceError, num::TryFromIntError};

use thiserror::Error;

#[derive(Error, Debug)]
pub enum DbError {
    #[error("Slice cast error")]
    SliceCast(#[from] TryFromSliceError),

    #[error("Int conversion error: ")]
    IntConversion(#[from] TryFromIntError),

    #[error("Io error:")]
    Io(#[from] std::io::Error),

    #[error("page not found")]
    PageNotFound,

    #[error("page full")]
    PageFull,

    #[error("db file not found")]
    FileNotFound,

    #[error("no more tuples")]
    NoMoreTuples,

    #[error("no table named `{0}`")]
    TableNotFound(String),

    #[error("corrupt page file")]
    CorruptPageFile,

    #[error("Input error: ")]
    InputError(#[from] DbInputError),

    #[error("no pages available")]
    NoPagesAvailable,

    #[error("incorrect page type")]
    PageCast,

    #[error("unknown error")]
    Unknown,
}

#[derive(Debug)]
pub enum DbInputError {
    StringTooLong,
    OutOfBounds,
}
impl std::fmt::Display for DbInputError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OutOfBounds => write!(f, "value out of bounds"),
            Self::StringTooLong => write!(f, "string exceeds maximum length"),
        }
    }
}

impl std::error::Error for DbInputError {}

pub type DbResult<T> = Result<T, DbError>;
