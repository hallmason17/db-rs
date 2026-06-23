/* Copyright (C) 2026 Mason Hall.
 *
 * GNU General Public License v3.0+ (see COPYING or https://www.gnu.org/licenses/gpl-3.0.txt)
 */
use std::str::Utf8Error;
use std::{array::TryFromSliceError, num::TryFromIntError, string::FromUtf8Error};
use thiserror::Error;

pub type Result<T> = core::result::Result<T, Error>;

#[derive(Error, Debug)]
pub enum Error {
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
    InputError(#[from] InputError),

    #[error("no pages available")]
    NoPagesAvailable,

    #[error("incorrect page type")]
    PageCast,

    #[error("invalid comparison")]
    InvalidComparison(String),

    #[error("parse error")]
    ParseError(String),

    #[error("couldn't convert from utf8")]
    Utf8Conversion(#[from] FromUtf8Error),
    #[error("couldn't convert to utf8")]
    Utf8Error(#[from] Utf8Error),

    #[error("unknown error")]
    Unknown,
}

#[derive(Debug)]
pub enum InputError {
    StringTooLong,
    OutOfBounds,
    RecordTooLarge,
    TupleDoesntMatchSchema,
}
impl std::fmt::Display for InputError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OutOfBounds => write!(f, "value out of bounds"),
            Self::StringTooLong => write!(f, "string exceeds maximum length"),
            Self::RecordTooLarge => {
                write!(f, "record exceeds maximum length (page_size - header_size)")
            }
            Self::TupleDoesntMatchSchema => {
                write!(f, "The given tuple doesn't match the table schema!")
            }
        }
    }
}

impl std::error::Error for InputError {}
