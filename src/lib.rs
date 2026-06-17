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
