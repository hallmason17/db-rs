use std::ops::{Deref, DerefMut};

use catalog::{Catalog, CatalogMut};
use fsm::{FreeSpaceMap, FreeSpaceMapMut};
use heap::{Heap, HeapMut};
use parking_lot::{RwLockReadGuard, RwLockWriteGuard};

use crate::PageGuard;
use crate::{DbError, PAGE_SIZE, page_header_offsets};

pub mod catalog;
pub mod fsm;
pub mod heap;

impl PageGuard<'_> {
    fn cast_read(
        &self,
        expected: PageKind,
    ) -> anyhow::Result<Page<RwLockReadGuard<'_, [u8; PAGE_SIZE]>>> {
        let data = self.handle.data.read();
        let page = Page { data };
        if page.header().kind() != expected {
            return Err(DbError::Unknown.into());
        }
        Ok(page)
    }
    fn cast_write(
        &mut self,
        expected: PageKind,
    ) -> anyhow::Result<Page<RwLockWriteGuard<'_, [u8; PAGE_SIZE]>>> {
        self.handle.frame.mark_dirty();
        let data = self.handle.data.write();
        let page = Page { data };
        if page.header().kind() != expected {
            return Err(DbError::Unknown.into());
        }
        Ok(page)
    }
    pub fn as_heap(&self) -> anyhow::Result<Heap<RwLockReadGuard<'_, [u8; PAGE_SIZE]>>> {
        let page = self.cast_read(PageKind::Heap)?;
        Ok(Heap { data: page.data })
    }
    pub fn as_heap_mut(
        &mut self,
    ) -> anyhow::Result<HeapMut<RwLockWriteGuard<'_, [u8; PAGE_SIZE]>>> {
        let page = self.cast_write(PageKind::Heap)?;
        Ok(HeapMut { data: page.data })
    }
    pub fn as_catalog(&self) -> anyhow::Result<Catalog<RwLockReadGuard<'_, [u8; PAGE_SIZE]>>> {
        let page = self.cast_read(PageKind::Catalog)?;
        Ok(Catalog { data: page.data })
    }

    pub fn as_catalog_mut(
        &mut self,
    ) -> anyhow::Result<CatalogMut<RwLockWriteGuard<'_, [u8; PAGE_SIZE]>>> {
        let page = self.cast_write(PageKind::Catalog)?;
        Ok(CatalogMut { data: page.data })
    }

    pub fn as_fsm(&self) -> anyhow::Result<FreeSpaceMap<RwLockReadGuard<'_, [u8; PAGE_SIZE]>>> {
        let page = self.cast_read(PageKind::FreeSpaceMap)?;
        Ok(FreeSpaceMap { data: page.data })
    }

    pub fn as_fsm_mut(
        &mut self,
    ) -> anyhow::Result<FreeSpaceMapMut<RwLockWriteGuard<'_, [u8; PAGE_SIZE]>>> {
        let page = self.cast_write(PageKind::FreeSpaceMap)?;
        Ok(FreeSpaceMapMut { data: page.data })
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
    fn get_slot_array_entry(&self, index: u16) -> anyhow::Result<Option<SlotArrayEntry>> {
        if index >= self.header().num_entries() {
            anyhow::bail!("out of bounds");
        }
        let offset = page_header_offsets::SIZE + size_of::<SlotArrayEntry>() * index as usize;
        let sa_offset = u16::from_be_bytes(self.data()[offset..offset + 2].try_into()?);
        let sa_size = u16::from_be_bytes(self.data()[offset + 2..offset + 4].try_into()?);
        Ok(Some(SlotArrayEntry {
            offset: sa_offset,
            size: sa_size,
        }))
    }
    fn get_freespace_start(&self) -> anyhow::Result<u16> {
        if self.header().num_entries() == 0 {
            Ok(u16::try_from(page_header_offsets::SIZE)?)
        } else {
            Ok(u16::try_from(page_header_offsets::SIZE)?
                + u16::try_from(size_of::<SlotArrayEntry>())? * self.header().num_entries())
        }
    }
    fn get_slot(&self, slot_index: u16) -> anyhow::Result<Option<&[u8]>> {
        if slot_index >= self.header().num_entries() {
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
}

pub trait SlottedPageMut: SlottedPage + PageAccessorMut {
    fn insert(&mut self, data: &[u8]) -> anyhow::Result<PageEntryId> {
        tracing::debug!("Inserting: ({:?})", data);
        let size = data.len();
        let num_entries = self.header().num_entries();
        let freespace_start = self.get_freespace_start()?;
        let new_freespace_start = freespace_start + u16::try_from(size_of::<SlotArrayEntry>())?;
        let offset = if num_entries > 0 {
            if let Some(sa_entry) = self.get_slot_array_entry(num_entries - 1)? {
                tracing::debug!("Entry found on page! {:?}, inserting after that!", sa_entry);
                sa_entry.offset - u16::try_from(size)?
            } else {
                anyhow::bail!("Failed to get entry that we checked existed! Possible corruption!")
            }
        } else {
            u16::try_from(PAGE_SIZE)? - u16::try_from(size)?
        };

        if offset < new_freespace_start {
            return Err(DbError::PageFull.into());
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

    fn get_slot_mut(&mut self, slot_index: u16) -> anyhow::Result<Option<&mut [u8]>> {
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
    data: G,
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
        bytes.copy_from_slice(&self.offset.to_be_bytes());
        bytes.copy_from_slice(&self.size.to_be_bytes());
        bytes
    }
}
