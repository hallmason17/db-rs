use std::{cell::RefCell, num::TryFromIntError};

use thiserror::Error;

pub mod buffer_pool;
pub mod catalog;
pub mod database;
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

#[allow(dead_code)]
#[derive(Debug)]
pub struct Frame {
    data: RefCell<[u8; PAGE_SIZE]>,
    pub state: RefCell<FrameState>,
}
#[derive(Debug)]
pub struct FrameState {
    pub page_id: Option<PageId>,
    pub pin_count: i32,
    pub clock_flag: bool,
    pub dirty: bool,
}
impl Frame {
    pub fn mark_dirty(&self) {
        self.state.borrow_mut().dirty = true;
    }
    pub fn unpin(&self) {
        let mut state = self.state.borrow_mut();
        state.pin_count -= 1;
        tracing::warn!("unpin page: {:?} -> {}", state.page_id, state.pin_count);
    }
    pub fn pin(&self) {
        let mut state = self.state.borrow_mut();
        state.pin_count += 1;
        state.clock_flag = true;
        tracing::warn!("pin page: {:?} -> {}", state.page_id, state.pin_count);
    }
}
impl Default for Frame {
    fn default() -> Self {
        let buf = RefCell::new([0u8; PAGE_SIZE]);
        Self {
            data: buf,
            state: RefCell::new(FrameState {
                page_id: None,
                pin_count: 0,
                clock_flag: false,
                dirty: false,
            }),
        }
    }
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct PageGuard<'pg> {
    page_id: PageId,
    frame: &'pg Frame,
}
impl Drop for PageGuard<'_> {
    fn drop(&mut self) {
        self.frame.unpin();
    }
}
