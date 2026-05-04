use std::sync::{Arc, RwLock};

use thiserror::Error;

mod buffer_mgr;
mod catalog;
mod storage_mgr;
mod tables;
pub(crate) const PAGE_SIZE: usize = 4096;

#[derive(PartialEq, Eq, Hash, Debug, Copy, Clone)]
pub struct PageId {
    pub file_id: u32,
    pub page_num: u32,
}

#[allow(dead_code)]
pub struct FrameHandle {
    page_id: PageId,
    data: Arc<RwLock<[u8; PAGE_SIZE]>>,
}

#[derive(Error, Debug)]
pub enum DbError {
    #[error("IO error:")]
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

    #[error("unknown error")]
    Unknown,
}

type DbResult<T> = Result<T, DbError>;
