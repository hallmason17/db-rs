use std::ops::{Deref, DerefMut};

use parking_lot::{RwLockReadGuard, RwLockWriteGuard};
use zerocopy::{Immutable, IntoBytes, KnownLayout, TryFromBytes, big_endian};

use crate::{PAGE_SIZE, PageGuard};

#[derive(Debug, Clone, Copy, PartialEq, Eq, IntoBytes, Immutable, TryFromBytes)]
#[repr(u8)]
pub enum PageKind {
    Heap = 0,
    Catalog = 1,
}

pub type HeapPage<'a> = Page<RwLockReadGuard<'a, [u8; PAGE_SIZE]>>;
pub type HeapPageMut<'a> = Page<RwLockWriteGuard<'a, [u8; PAGE_SIZE]>>;

pub type CatalogPage<'a> = HeapPage<'a>;
pub type CatalogPageMut<'a> = HeapPageMut<'a>;

impl<'a> PageGuard<'a> {
    // TODO: check the kind byte for type safety
    pub fn as_heap(&self) -> HeapPage<'_> {
        let data = self.handle.data.read();
        HeapPage { data }
    }
    pub fn as_heap_mut(&mut self) -> HeapPageMut<'_> {
        self.handle.frame.mark_dirty();
        let data = self.handle.data.write();
        HeapPageMut { data }
    }
    pub fn as_catalog(&self) -> CatalogPage<'_> {
        let data = self.handle.data.read();
        CatalogPage { data }
    }

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

#[derive(Debug, Clone, IntoBytes, TryFromBytes, Immutable, KnownLayout)]
#[repr(C, packed(1))]
pub struct PageHeader {
    pub kind: PageKind,
    pub page_id: big_endian::U64,
    pub num_entries: big_endian::U16,
}
impl PageHeader {
    /*
    pub fn to_be_bytes(self) -> [u8; size_of::<Self>()] {
        let mut bytes = [0u8; size_of::<Self>()];
        bytes[0..8].copy_from_slice(&self.page_id.to_be_bytes());
        bytes[8] = match self.kind {
            PageKind::Heap => 0,
            PageKind::Catalog => 0,
        };
        bytes[9..11].copy_from_slice(&self.num_entries.to_be_bytes());
        bytes
    }
    */
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
    // casting directly to a header like i do in C
    fn header_mut(&mut self) -> &mut PageHeader {
        PageHeader::try_mut_from_prefix(&mut self.data[..])
            .expect("invalid header")
            .0
    }

    pub fn insert(&mut self, data: &[u8]) -> anyhow::Result<PageEntryId> {
        tracing::debug!("{:?}", self.header());
        let size = data.len();
        let num_entries = self.num_entries()?;
        let freespace_start = self.get_freespace_start()?;
        let new_freespace_start = freespace_start + size_of::<SlotArrayEntry>() as u16;
        let offset = if num_entries > 0 {
            let sa_entry = self.get_slot_array_entry(num_entries - 1)?.unwrap();
            tracing::debug!("Entry found on page! {:?}", sa_entry);
            sa_entry.offset - size as u16
        } else {
            PAGE_SIZE as u16
        };

        if offset < new_freespace_start {
            anyhow::bail!("page full!");
        }

        let new_sa_entry = SlotArrayEntry {
            offset,
            size: size as u16,
        };
        self.data[freespace_start as usize..new_freespace_start as usize]
            .copy_from_slice(&new_sa_entry.to_be_bytes());

        self.data[offset as usize - size..offset as usize].copy_from_slice(&data);

        let header = self.header_mut();
        header.num_entries.set(num_entries + 1);
        Ok(PageEntryId {
            page: self.id()?,
            slot: num_entries,
        })
    }

    pub fn get_slot_mut(&mut self, slot_index: u16) -> anyhow::Result<Option<&mut [u8]>> {
        if slot_index >= self.num_entries()? {
            return Ok(None);
        }
        let sa_offset = size_of::<PageHeader>() + size_of::<SlotArrayEntry>() * slot_index as usize;
        let data_offset = u16::from_be_bytes(self.data[sa_offset..sa_offset + 2].try_into()?);
        let data_size = u16::from_be_bytes(self.data[sa_offset + 2..sa_offset + 4].try_into()?);
        Ok(Some(
            &mut self.data[data_offset as usize..(data_offset + data_size) as usize],
        ))
    }
}

impl<G> Page<G>
where
    G: Deref<Target = [u8; PAGE_SIZE]>,
{
    pub fn num_entries(&self) -> anyhow::Result<u16> {
        Ok(self.header().num_entries.get())
    }
    pub fn id(&self) -> anyhow::Result<u64> {
        Ok(self.header().page_id.get())
    }
    pub fn kind(&self) -> anyhow::Result<PageKind> {
        Ok(self.header().kind)
    }

    fn header(&self) -> &PageHeader {
        PageHeader::try_ref_from_prefix(&self.data[..])
            .expect("invalid header")
            .0
    }

    fn get_slot_array_entry(&self, index: u16) -> anyhow::Result<Option<SlotArrayEntry>> {
        if index >= self.num_entries()? {
            anyhow::bail!("out of bounds");
        }
        let offset = size_of::<PageHeader>() + size_of::<SlotArrayEntry>() * index as usize;
        let sa_offset = u16::from_be_bytes(self.data.as_ref()[offset..offset + 2].try_into()?);
        let sa_size = u16::from_be_bytes(self.data.as_ref()[offset + 2..offset + 4].try_into()?);
        Ok(Some(SlotArrayEntry {
            offset: sa_offset,
            size: sa_size,
        }))
    }
    fn get_freespace_start(&self) -> anyhow::Result<u16> {
        if self.num_entries()? == 0 {
            Ok(size_of::<PageHeader>() as u16)
        } else {
            Ok(size_of::<PageHeader>() as u16
                + size_of::<SlotArrayEntry>() as u16 * self.num_entries()?)
        }
    }
    pub fn get_slot(&self, slot_index: u16) -> anyhow::Result<Option<&[u8]>> {
        if slot_index >= self.num_entries()? {
            return Ok(None);
        }
        let sa_offset = size_of::<PageHeader>() + size_of::<SlotArrayEntry>() * slot_index as usize;
        let data_offset = u16::from_be_bytes(self.data[sa_offset..sa_offset + 2].try_into()?);
        let data_size = u16::from_be_bytes(self.data[sa_offset + 2..sa_offset + 4].try_into()?);
        Ok(Some(
            &self.data[data_offset as usize..(data_offset + data_size) as usize],
        ))
    }
}
