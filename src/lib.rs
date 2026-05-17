use parking_lot::RwLock;
use std::{num::TryFromIntError, sync::Arc};

use thiserror::Error;

use crate::{buffer_pool::Frame, page::PageKind};

pub mod buffer_pool;
pub mod catalog;
pub mod page;
pub mod storage;
pub mod tables;
pub(crate) mod page_header_offsets {
    pub const ID: usize = 0;
    pub const KIND: usize = 8;
    pub const ENTRIES: usize = 9;
    pub const NEXT_PAGE: usize = 11;
    pub const SIZE: usize = 19;
    pub(crate) mod header_page {
        pub const FIRST_FREE_PAGE_ID: usize = 19;
    }
    pub(crate) mod fsm_page {
        pub const FSM_NUM: usize = 21;
        pub const SIZE: usize = 23;
    }
}
pub(crate) const PAGE_SIZE: usize = 4096;
pub(crate) const MAGIC_NUMBER: u64 = 0xDBDB_DBDB;
pub(crate) const CATALOG_PAGE_ID: PageId = PageId {
    file_id: 0,
    page_num: 0,
};
pub(crate) const INITIAL_FIRST_FREE_PAGE_NUMBER: usize = 2;

#[derive(Debug, Copy, Clone)]
#[repr(C)]
pub struct PageFileFooter {
    magic_number: u64,
    num_pages: u64,
}

impl PageFileFooter {
    #[must_use]
    pub fn from_be_bytes(bytes: &[u8]) -> Self {
        let magic_number = u64::from_be_bytes(bytes[..8].try_into().unwrap());
        let num_pages = u64::from_be_bytes(bytes[8..8 + 8].try_into().unwrap());
        Self {
            magic_number,
            num_pages,
        }
    }
    #[must_use]
    pub fn to_be_bytes(self) -> [u8; size_of::<Self>()] {
        let mut bytes = [0u8; size_of::<Self>()];
        bytes[0..8].copy_from_slice(&self.magic_number.to_be_bytes());
        bytes[8..16].copy_from_slice(&self.num_pages.to_be_bytes());
        bytes
    }
}

#[derive(PartialEq, Eq, Hash, Debug, Copy, Clone)]
pub struct PageId {
    pub file_id: u32,
    pub page_num: u64,
}

#[derive(Error, Debug)]
pub enum DbError {
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
    #[error("unknown error")]
    Unknown,
}

#[derive(Debug)]
pub enum DbInputError {
    StringTooLong,
    OutOfBounds,
}
impl std::fmt::Display for DbInputError {
    fn fmt(&self, _f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
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
    let next_page: usize = 0;
    data[page_header_offsets::ID..page_header_offsets::ID + 8]
        .copy_from_slice(&page_id.to_be_bytes());
    data[page_header_offsets::KIND] = kind as u8;
    data[page_header_offsets::ENTRIES..page_header_offsets::ENTRIES + 2]
        .copy_from_slice(&num_entries.to_be_bytes());
    data[page_header_offsets::NEXT_PAGE..page_header_offsets::NEXT_PAGE + 8]
        .copy_from_slice(&next_page.to_be_bytes());
    match kind {
        PageKind::Catalog => {
            data[page_header_offsets::header_page::FIRST_FREE_PAGE_ID
                ..page_header_offsets::header_page::FIRST_FREE_PAGE_ID + 8]
                .copy_from_slice(&INITIAL_FIRST_FREE_PAGE_NUMBER.to_be_bytes());
        }
        _ => {}
    }
    data
}
