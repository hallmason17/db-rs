use parking_lot::RwLock;
use std::{num::TryFromIntError, sync::Arc};
use zerocopy::{FromBytes, Immutable, IntoBytes, big_endian};

use thiserror::Error;

use crate::{
    buffer_pool::Frame,
    page::{PageHeaderView, PageKind},
};

pub mod buffer_pool;
pub mod catalog;
pub mod page;
pub mod storage;
pub mod tables;
pub mod header_offsets {
    pub const ID: usize = 0;
    pub const KIND: usize = 8;
    pub const ENTRIES: usize = 9;
}
pub(crate) const PAGE_SIZE: usize = 4096;
pub(crate) const MAGIC_NUMBER: u64 = 0xDBDBDBDB;
pub(crate) const CATALOG_PAGE_ID: PageId = PageId {
    file_id: 0,
    page_num: 0,
};

#[derive(IntoBytes, FromBytes, Immutable)]
#[repr(C)]
pub struct PageFileFooter {
    magic_number: big_endian::U64,
    num_pages: big_endian::U32,
}

#[derive(PartialEq, Eq, Hash, Debug, Copy, Clone)]
pub struct PageId {
    pub file_id: u32,
    pub page_num: u32,
}

#[derive(Error, Debug)]
pub enum DbError {
    #[error("Int conversion error: ")]
    IntConversion(#[from] TryFromIntError),
    #[error("Io error:")]
    Io(#[from] std::io::Error),

    #[error("page not found")]
    PageNotFound,

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
        Ok(())
    }
}

impl std::error::Error for DbInputError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        None
    }

    fn description(&self) -> &str {
        "description() is deprecated; use Display"
    }

    fn cause(&self) -> Option<&dyn std::error::Error> {
        self.source()
    }
}

type DbResult<T> = Result<T, DbError>;

#[derive(Debug)]
#[allow(dead_code)]
pub struct FrameHandle<'a> {
    pub page_id: PageId,
    pub data: Arc<RwLock<[u8; PAGE_SIZE]>>,
    frame: &'a Frame,
}

pub struct PageGuard<'a> {
    handle: FrameHandle<'a>,
}

fn create_blank_page(page_id: u64, kind: PageKind) -> [u8; PAGE_SIZE] {
    let mut data = [0u8; PAGE_SIZE];
    let num_entries: u16 = 0;
    data[header_offsets::ID..header_offsets::ID + 8].copy_from_slice(&page_id.to_be_bytes());
    data[header_offsets::KIND] = kind as u8;
    data[header_offsets::ENTRIES..header_offsets::ENTRIES + 2]
        .copy_from_slice(&num_entries.to_be_bytes());
    data
}
