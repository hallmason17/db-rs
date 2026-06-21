/* Copyright (C) 2026 Mason Hall.
 *
 * GNU General Public License v3.0+ (see COPYING or https://www.gnu.org/licenses/gpl-3.0.txt)
 */
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Copy, Clone)]
pub struct FileId(pub u32);

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Copy, Clone)]
pub struct TableId(pub u32);

#[derive(PartialEq, Eq, Hash, Debug, Copy, Clone)]
pub struct PageId {
    pub file_id: FileId,
    pub page_num: u64,
}
