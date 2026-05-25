use std::{
    cell::{Ref, RefMut},
    ops::{Deref, DerefMut},
};

use parking_lot::{RwLockReadGuard, RwLockWriteGuard};

use crate::{
<<<<<<< HEAD
    PAGE_SIZE, PageGuard,
=======
>>>>>>> main
    page::{PageAccessor, PageAccessorMut, SlottedPage, SlottedPageMut},
    PageGuard, PAGE_SIZE,
};

use super::PageKind;

pub struct Heap<G> {
    pub data: G,
}
pub struct HeapMut<G> {
    pub data: G,
}
impl<G> HeapMut<G> where G: DerefMut<Target = [u8; PAGE_SIZE]> {}
impl<G> PageAccessor for Heap<G>
where
    G: Deref<Target = [u8; PAGE_SIZE]>,
{
    fn data(&self) -> &[u8; PAGE_SIZE] {
        &self.data
    }
}
impl<G> PageAccessor for HeapMut<G>
where
    G: Deref<Target = [u8; PAGE_SIZE]>,
{
    fn data(&self) -> &[u8; PAGE_SIZE] {
        &self.data
    }
}
impl<G> PageAccessorMut for HeapMut<G>
where
    G: DerefMut<Target = [u8; PAGE_SIZE]>,
{
    fn data_mut(&mut self) -> &mut [u8; PAGE_SIZE] {
        &mut self.data
    }
}

impl<G> SlottedPage for Heap<G> where G: Deref<Target = [u8; PAGE_SIZE]> {}
impl<G> SlottedPage for HeapMut<G> where G: Deref<Target = [u8; PAGE_SIZE]> {}
impl<G> SlottedPageMut for HeapMut<G> where G: DerefMut<Target = [u8; PAGE_SIZE]> {}

impl PageGuard<'_> {
<<<<<<< HEAD
    pub fn with_heap_mut<T>(
        &mut self,
        f: impl FnOnce(&mut HeapMut<RefMut<'_, [u8;PAGE_SIZE]>>) -> anyhow::Result<T>,
    ) -> anyhow::Result<T> {
        let page = self.cast_write(PageKind::Heap)?;
        f(&mut HeapMut{data: page.data})
    }
    pub fn as_heap(&self) -> anyhow::Result<Heap<Ref<'_, [u8; PAGE_SIZE]>>> {
        let page = self.cast_read(PageKind::Heap)?;
        Ok(Heap { data: page.data })
    }
    pub fn as_heap_mut(&mut self) -> anyhow::Result<HeapMut<RefMut<'_, [u8; PAGE_SIZE]>>> {
=======
    pub fn as_heap(&self) -> anyhow::Result<Heap<RwLockReadGuard<'_, [u8; PAGE_SIZE]>>> {
        let page = self.cast_read(PageKind::Heap)?;
        Ok(Heap { data: page.data })
    }
    pub fn as_heap_mut(
        &mut self,
    ) -> anyhow::Result<HeapMut<RwLockWriteGuard<'_, [u8; PAGE_SIZE]>>> {
>>>>>>> main
        let page = self.cast_write(PageKind::Heap)?;
        Ok(HeapMut { data: page.data })
    }
}
