pub mod buffer_pool;
pub mod catalog;
pub mod database;
pub mod error;
pub mod execution;
pub mod expr;
pub mod ids;
pub mod page;
pub mod planner;
pub mod server;
pub mod sql;
pub mod storage;
pub mod tables;
pub mod transaction;
pub mod value;

pub use buffer_pool::PageGuard;

use crate::ids::{FileId, PageId};

pub(crate) const MAGIC_NUMBER: u64 = 0xDBDB_DBDB;
pub(crate) const CATALOG_PAGE_ID: PageId = PageId {
    file_id: FileId(0),
    page_num: 0,
};
pub(crate) const INITIAL_FIRST_FREE_PAGE_NUMBER: u64 = 2;

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
