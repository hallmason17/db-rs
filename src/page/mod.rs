use std::cell::{Ref, RefMut};
use std::ops::{Deref, DerefMut};

use crate::error::{DbError, DbInputError, DbResult};
use crate::{INITIAL_FIRST_FREE_PAGE_NUMBER, PageGuard};

pub mod catalog;
pub mod fsm;
pub mod heap;

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

pub fn create_blank_page(page_id: u64, kind: PageKind) -> [u8; PAGE_SIZE] {
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
    if kind == PageKind::Catalog {
        data[page_header_offsets::header_page::FIRST_FREE_PAGE_ID
            ..page_header_offsets::header_page::FIRST_FREE_PAGE_ID + 8]
            .copy_from_slice(&INITIAL_FIRST_FREE_PAGE_NUMBER.to_be_bytes());
    }
    data
}

impl PageGuard<'_> {
    fn cast_read(&self, expected: PageKind) -> DbResult<Page<Ref<'_, [u8; PAGE_SIZE]>>> {
        let data = self.borrow_data();
        let page = Page { data };
        if page.header().kind() != expected {
            tracing::error!(
                "Tried to cast page {:?} to {:?}, it's a {:?}",
                self.page_id,
                page.header().kind(),
                expected
            );
            tracing::error!("Data: {:?}", page.data);
            return Err(DbError::PageCast);
        }
        Ok(page)
    }
    fn cast_write(&mut self, expected: PageKind) -> DbResult<Page<RefMut<'_, [u8; PAGE_SIZE]>>> {
        let data = self.borrow_data_mut();
        let page = Page { data };
        if page.header().kind() != expected {
            tracing::error!(
                "Tried to cast page {:?} to {:?}, it's a {:?}",
                page.header().page_id(),
                page.header().kind(),
                expected
            );
            return Err(DbError::PageCast);
        }
        Ok(page)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PageKind {
    Heap = 0,
    Catalog = 1,
    FreeSpaceMap = 2,
}

pub trait PageAccessor {
    fn header(&self) -> PageHeaderView<'_> {
        PageHeaderView(&self.data()[..page_header_offsets::SIZE])
    }
    fn data(&self) -> &[u8; PAGE_SIZE];
}
pub trait PageAccessorMut: PageAccessor {
    fn header_mut(&mut self) -> PageHeaderMut<'_> {
        PageHeaderMut(&mut self.data_mut()[..page_header_offsets::SIZE])
    }
    fn data_mut(&mut self) -> &mut [u8; PAGE_SIZE];
}

pub trait SlottedPage: PageAccessor {
    fn get_slot_array_entry(&self, index: u16) -> DbResult<Option<SlotArrayEntry>> {
        if index >= self.num_entries() {
            return Err(DbError::InputError(DbInputError::OutOfBounds));
        }
        let offset = page_header_offsets::SIZE + size_of::<SlotArrayEntry>() * index as usize;
        let sa_offset = u16::from_be_bytes(self.data()[offset..offset + 2].try_into()?);
        let sa_size = u16::from_be_bytes(self.data()[offset + 2..offset + 4].try_into()?);
        Ok(Some(SlotArrayEntry {
            offset: sa_offset,
            size: sa_size,
        }))
    }
    fn get_freespace_start(&self) -> DbResult<u16> {
        if self.num_entries() == 0 {
            Ok(u16::try_from(page_header_offsets::SIZE)?)
        } else {
            Ok(u16::try_from(page_header_offsets::SIZE)?
                + u16::try_from(size_of::<SlotArrayEntry>())? * self.num_entries())
        }
    }
    fn get_slot(&self, slot_index: u16) -> DbResult<Option<&[u8]>> {
        if slot_index >= self.num_entries() {
            return Ok(None);
        }
        let sa_offset =
            page_header_offsets::SIZE + size_of::<SlotArrayEntry>() * slot_index as usize;
        let data_offset = u16::from_be_bytes(self.data()[sa_offset..sa_offset + 2].try_into()?);
        let data_size = u16::from_be_bytes(self.data()[sa_offset + 2..sa_offset + 4].try_into()?);
        Ok(Some(
            &self.data()[data_offset as usize..(data_offset + data_size) as usize],
        ))
    }

    fn num_entries(&self) -> u16 {
        self.header().num_entries()
    }
}

pub trait SlottedPageMut: SlottedPage + PageAccessorMut {
    fn insert(&mut self, data: &[u8]) -> DbResult<PageEntryId> {
        tracing::debug!("Inserting: ({:?})", data);
        let size = data.len();
        let num_entries = self.num_entries();
        let freespace_start = self.get_freespace_start()?;
        let new_freespace_start = freespace_start + u16::try_from(size_of::<SlotArrayEntry>())?;
        let offset = if num_entries > 0 {
            if let Some(sa_entry) = self.get_slot_array_entry(num_entries - 1)? {
                tracing::debug!(
                    "Entry found on page at slot {:?}! {:?}, inserting after that!",
                    num_entries - 1,
                    sa_entry
                );
                sa_entry.offset - u16::try_from(size)?
            } else {
                return Err(DbError::CorruptPageFile);
            }
        } else {
            u16::try_from(PAGE_SIZE)? - u16::try_from(size)?
        };

        if offset < new_freespace_start {
            return Err(DbError::PageFull);
        }

        let new_sa_entry = SlotArrayEntry {
            offset,
            size: u16::try_from(size)?,
        };

        tracing::debug!("Creating new slot_array_entry: {:?}", new_sa_entry);
        self.data_mut()[freespace_start as usize..new_freespace_start as usize]
            .copy_from_slice(&new_sa_entry.to_be_bytes());

        self.data_mut()[offset as usize..(offset as usize + size)].copy_from_slice(data);

        let mut header = self.header_mut();
        header.set_num_entries(num_entries + 1);
        Ok(PageEntryId {
            page: self.header().page_id(),
            slot: num_entries,
        })
    }

    fn get_slot_mut(&mut self, slot_index: u16) -> DbResult<Option<&mut [u8]>> {
        if slot_index >= self.header().num_entries() {
            return Ok(None);
        }

        let sa_offset =
            page_header_offsets::SIZE + size_of::<SlotArrayEntry>() * slot_index as usize;
        let data_offset = u16::from_be_bytes(self.data()[sa_offset..sa_offset + 2].try_into()?);
        let data_size = u16::from_be_bytes(self.data()[sa_offset + 2..sa_offset + 4].try_into()?);
        tracing::debug!(
            "Getting slot at offset: {} of size: {}",
            data_offset,
            data_size
        );
        Ok(Some(
            &mut self.data_mut()[data_offset as usize..(data_offset + data_size) as usize],
        ))
    }
}

#[derive(Debug, Clone)]
#[repr(C)]
pub struct PageEntryId {
    pub page: u64,
    pub slot: u16,
}

#[derive(Debug)]
pub struct PageHeaderView<'a>(&'a [u8]);
pub struct PageHeaderMut<'a>(&'a mut [u8]);
impl<'a> PageHeaderMut<'a> {
    pub fn new(data: &'a mut [u8]) -> Self {
        Self(data)
    }
}
pub trait PageHeaderReader {
    fn data(&self) -> &[u8];

    fn page_id(&self) -> u64 {
        u64::from_be_bytes(
            self.data()[page_header_offsets::ID..page_header_offsets::ID + 8]
                .try_into()
                .unwrap(),
        )
    }

    fn kind(&self) -> PageKind {
        match self.data()[page_header_offsets::KIND] {
            0 => PageKind::Heap,
            1 => PageKind::Catalog,
            2 => PageKind::FreeSpaceMap,
            _ => unreachable!(),
        }
    }

    fn num_entries(&self) -> u16 {
        u16::from_be_bytes(
            self.data()[page_header_offsets::ENTRIES..page_header_offsets::ENTRIES + 2]
                .try_into()
                .unwrap(),
        )
    }

    fn next_page(&self) -> u64 {
        u64::from_be_bytes(
            self.data()[page_header_offsets::NEXT_PAGE..page_header_offsets::NEXT_PAGE + 8]
                .try_into()
                .unwrap(),
        )
    }
}
impl PageHeaderReader for PageHeaderView<'_> {
    fn data(&self) -> &[u8] {
        self.0
    }
}
impl PageHeaderReader for PageHeaderMut<'_> {
    fn data(&self) -> &[u8] {
        self.0
    }
}
impl PageHeaderMut<'_> {
    pub fn set_num_entries(&mut self, val: u16) {
        self.0[page_header_offsets::ENTRIES..page_header_offsets::ENTRIES + 2]
            .copy_from_slice(&val.to_be_bytes());
    }
    pub fn set_next_page(&mut self, val: u64) {
        self.0[page_header_offsets::NEXT_PAGE..page_header_offsets::NEXT_PAGE + 8]
            .copy_from_slice(&val.to_be_bytes());
    }
    pub fn set_kind(&mut self, val: PageKind) {
        self.0[page_header_offsets::KIND] = val as u8;
    }
    pub fn set_page_id(&mut self, val: u64) {
        self.0[page_header_offsets::ID..page_header_offsets::ID + 8]
            .copy_from_slice(&val.to_be_bytes());
    }
}

#[derive(Debug)]
pub struct Page<G> {
    pub data: G,
}

impl<G> PageAccessor for Page<G>
where
    G: Deref<Target = [u8; PAGE_SIZE]>,
{
    fn data(&self) -> &[u8; PAGE_SIZE] {
        &self.data
    }
}
impl<G> PageAccessorMut for Page<G>
where
    G: DerefMut<Target = [u8; PAGE_SIZE]>,
{
    fn data_mut(&mut self) -> &mut [u8; PAGE_SIZE] {
        &mut self.data
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct SlotArrayEntry {
    pub offset: u16,
    pub size: u16,
}
impl SlotArrayEntry {
    fn to_be_bytes(self) -> [u8; size_of::<SlotArrayEntry>()] {
        let mut bytes = [0u8; size_of::<SlotArrayEntry>()];
        bytes[..2].copy_from_slice(&self.offset.to_be_bytes());
        bytes[2..].copy_from_slice(&self.size.to_be_bytes());
        bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockSlottedPage {
        data: [u8; PAGE_SIZE],
    }
    impl MockSlottedPage {
        fn new(page_id: u64, kind: PageKind) -> Self {
            let mut data = [0u8; PAGE_SIZE];
            let mut header = PageHeaderMut::new(&mut data[..page_header_offsets::SIZE]);
            header.set_page_id(page_id);
            header.set_kind(kind);
            header.set_num_entries(0);
            Self { data }
        }
    }
    impl PageAccessor for MockSlottedPage {
        fn data(&self) -> &[u8; PAGE_SIZE] {
            &self.data
        }
    }
    impl PageAccessorMut for MockSlottedPage {
        fn data_mut(&mut self) -> &mut [u8; PAGE_SIZE] {
            &mut self.data
        }
    }
    impl SlottedPage for MockSlottedPage {}
    impl SlottedPageMut for MockSlottedPage {}

    #[test]
    fn insert_then_read_back() {
        let mut page = MockSlottedPage::new(0, PageKind::Heap);
        let payload = b"hello, world!";

        let eid = page.insert(payload).unwrap();
        assert_eq!(eid.page, 0);
        assert_eq!(eid.slot, 0);

        let read = page.get_slot(0).unwrap().unwrap();
        assert_eq!(read, payload);
    }

    #[test]
    fn insert_multiple_preserves_order() {
        let mut page = MockSlottedPage::new(0, PageKind::Heap);

        let eid0 = page.insert(b"first").unwrap();
        let eid1 = page.insert(b"second").unwrap();
        let eid2 = page.insert(b"third").unwrap();

        assert_eq!(eid0.slot, 0);
        assert_eq!(eid1.slot, 1);
        assert_eq!(eid2.slot, 2);

        assert_eq!(page.get_slot(0).unwrap().unwrap(), b"first");
        assert_eq!(page.get_slot(1).unwrap().unwrap(), b"second");
        assert_eq!(page.get_slot(2).unwrap().unwrap(), b"third");
    }

    #[test]
    fn page_full_when_slot_array_meets_data() {
        let mut page = MockSlottedPage::new(0, PageKind::Heap);

        let big = vec![0u8; PAGE_SIZE - page_header_offsets::SIZE - size_of::<SlotArrayEntry>()];
        let result = page.insert(&big);
        assert!(result.is_ok(), "should fit exactly once");

        let one_byte_too_many = vec![0u8; 1];
        let result = page.insert(&one_byte_too_many);
        assert!(result.is_err(), "should be full");
    }

    #[test]
    fn get_slot_out_of_bounds_returns_none() {
        let page = MockSlottedPage::new(0, PageKind::Heap);
        assert!(page.get_slot(0).unwrap().is_none());
    }

    #[test]
    fn get_slot_array_entry_out_of_bounds() {
        let page = MockSlottedPage::new(0, PageKind::Heap);
        let result = page.get_slot_array_entry(0);
        assert!(result.is_err());
    }
}
