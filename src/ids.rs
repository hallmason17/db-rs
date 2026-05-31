#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Copy, Clone)]
pub struct FileId(pub u32);

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Copy, Clone)]
pub struct TableId(pub u32);

#[derive(PartialEq, Eq, Hash, Debug, Copy, Clone)]
pub struct PageId {
    pub file_id: FileId,
    pub page_num: u64,
}
