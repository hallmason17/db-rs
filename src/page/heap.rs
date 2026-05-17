use std::ops::{Deref, DerefMut};

use crate::{
    PAGE_SIZE,
    page::{PageAccessor, PageAccessorMut, SlottedPage, SlottedPageMut},
};

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
