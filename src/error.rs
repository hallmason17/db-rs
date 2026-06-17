/* Copyright (C) 2026 Mason Hall.
 *
 * This file is part of db-rs.
 *
 * db-rs is free software: you can redistribute it and/or modify it under the
 * terms of the GNU General Public License as published by the Free Software
 * Foundation, either version 3 of the License, or (at your option) any later
 * version.
 *
 * db-rs is distributed in the hope that it will be useful, but WITHOUT ANY
 * WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS
 * FOR A PARTICULAR PURPOSE. See the GNU General Public License for more
 * details.
 *
 * You should have received a copy of the GNU General Public License along with
 * db-rs. If not, see <https://www.gnu.org/licenses/>.
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

    #[error("unknown error")]
    Unknown,
}

#[derive(Debug)]
pub enum InputError {
    StringTooLong,
    OutOfBounds,
    RecordTooLarge,
}
impl std::fmt::Display for InputError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OutOfBounds => write!(f, "value out of bounds"),
            Self::StringTooLong => write!(f, "string exceeds maximum length"),
            Self::RecordTooLarge => {
                write!(f, "record exceeds maximum length (page_size - header_size)")
            }
        }
    }
}

impl std::error::Error for InputError {}
