use std::ops::{Deref, DerefMut};

use parking_lot::{RwLockReadGuard, RwLockWriteGuard};

use crate::{page_header_offsets, PageGuard, PAGE_SIZE};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PageKind {
    Heap = 0,
    Catalog = 1,
}

pub type HeapPage<'a> = Page<RwLockReadGuard<'a, [u8; PAGE_SIZE]>>;
pub type HeapPageMut<'a> = Page<RwLockWriteGuard<'a, [u8; PAGE_SIZE]>>;

pub type CatalogPage<'a> = HeapPage<'a>;
pub type CatalogPageMut<'a> = HeapPageMut<'a>;

impl PageGuard<'_> {
    // TODO: check the kind byte for type safety
    #[must_use]
    pub fn as_heap(&self) -> HeapPage<'_> {
        let data = self.handle.data.read();
        HeapPage { data }
    }
    #[must_use]
    pub fn as_heap_mut(&mut self) -> HeapPageMut<'_> {
        self.handle.frame.mark_dirty();
        let data = self.handle.data.write();
        HeapPageMut { data }
    }
    #[must_use]
    pub fn as_catalog(&self) -> CatalogPage<'_> {
        let data = self.handle.data.read();
        CatalogPage { data }
    }

    #[must_use]
    pub fn as_catalog_mut(&mut self) -> CatalogPageMut<'_> {
        self.handle.frame.mark_dirty();
        let data = self.handle.data.write();
        CatalogPageMut { data }
    }
}

#[derive(Debug, Clone)]
#[repr(C)]
pub struct PageEntryId {
    page: u64,
    slot: u16,
}

#[derive(Debug)]
pub struct PageHeaderView<'a>(&'a [u8]);
pub struct PageHeaderMut<'a>(&'a mut [u8]);
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
}

#[derive(Debug)]
pub struct Page<G> {
    data: G,
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct SlotArrayEntry {
    pub offset: u16,
    pub size: u16,
}
impl SlotArrayEntry {
    #[must_use]
    pub fn to_be_bytes(self) -> [u8; size_of::<SlotArrayEntry>()] {
        // TODO: no copying?
        let mut bytes = [0u8; size_of::<SlotArrayEntry>()];
        bytes[0..2].copy_from_slice(&self.offset.to_be_bytes());
        bytes[2..4].copy_from_slice(&self.size.to_be_bytes());
        bytes
    }
}

impl<G> Page<G>
where
    G: DerefMut<Target = [u8; PAGE_SIZE]>,
{
    /*
    // casting directly to a header like i do in C
    fn header_mut(&mut self) -> &mut PageHeader {
        PageHeader::try_mut_from_prefix(&mut self.data[..])
            .expect("invalid header")
            .0
    }
     */

    fn header_mut(&mut self) -> PageHeaderMut<'_> {
        PageHeaderMut(&mut self.data[..])
    }

    pub fn insert(&mut self, data: &[u8]) -> anyhow::Result<PageEntryId> {
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
            anyhow::bail!("page full!");
        }

        let new_sa_entry = SlotArrayEntry {
            offset,
            size: u16::try_from(size)?,
        };
        tracing::debug!("Creating new slot_array_entry: {:?}", new_sa_entry);
        self.data[freespace_start as usize..new_freespace_start as usize]
            .copy_from_slice(&new_sa_entry.to_be_bytes());

        self.data[offset as usize..(offset as usize + size)].copy_from_slice(data);

        let mut header = self.header_mut();
        header.set_num_entries(num_entries + 1);
        Ok(PageEntryId {
            page: self.header().page_id(),
            slot: num_entries,
        })
    }

    pub fn get_slot_mut(&mut self, slot_index: u16) -> anyhow::Result<Option<&mut [u8]>> {
        if slot_index >= self.header().num_entries() {
            return Ok(None);
        }
        let sa_offset =
            size_of::<PageHeaderView>() + size_of::<SlotArrayEntry>() * slot_index as usize;
        let data_offset = u16::from_be_bytes(self.data[sa_offset..sa_offset + 2].try_into()?);
        let data_size = u16::from_be_bytes(self.data[sa_offset + 2..sa_offset + 4].try_into()?);
        tracing::debug!(
            "Getting slot at offset: {} of size: {}",
            data_offset,
            data_size
        );
        Ok(Some(
            &mut self.data[data_offset as usize..(data_offset + data_size) as usize],
        ))
    }
}

impl<G> Page<G>
where
    G: Deref<Target = [u8; PAGE_SIZE]>,
{
    pub fn header(&self) -> PageHeaderView<'_> {
        PageHeaderView(self.data.as_ref())
    }

    fn get_slot_array_entry(&self, index: u16) -> anyhow::Result<Option<SlotArrayEntry>> {
        if index >= self.header().num_entries() {
            anyhow::bail!("out of bounds");
        }
        let offset = size_of::<PageHeaderView>() + size_of::<SlotArrayEntry>() * index as usize;
        let sa_offset = u16::from_be_bytes(self.data.as_ref()[offset..offset + 2].try_into()?);
        let sa_size = u16::from_be_bytes(self.data.as_ref()[offset + 2..offset + 4].try_into()?);
        Ok(Some(SlotArrayEntry {
            offset: sa_offset,
            size: sa_size,
        }))
    }
    fn get_freespace_start(&self) -> anyhow::Result<u16> {
        if self.header().num_entries() == 0 {
            Ok(u16::try_from(size_of::<PageHeaderView>())?)
        } else {
            Ok(u16::try_from(size_of::<PageHeaderView>())?
                + u16::try_from(size_of::<SlotArrayEntry>())? * self.header().num_entries())
        }
    }
    pub fn get_slot(&self, slot_index: u16) -> anyhow::Result<Option<&[u8]>> {
        if slot_index >= self.header().num_entries() {
            return Ok(None);
        }
        let sa_offset =
            size_of::<PageHeaderView>() + size_of::<SlotArrayEntry>() * slot_index as usize;
        let data_offset = u16::from_be_bytes(self.data[sa_offset..sa_offset + 2].try_into()?);
        let data_size = u16::from_be_bytes(self.data[sa_offset + 2..sa_offset + 4].try_into()?);
        Ok(Some(
            &self.data[data_offset as usize..(data_offset + data_size) as usize],
        ))
    }
}
