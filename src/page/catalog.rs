use std::ops::{Deref, DerefMut};

use crate::{
    PAGE_SIZE,
    page::{PageAccessor, PageAccessorMut, SlottedPage, SlottedPageMut},
};

pub struct Catalog<G> {
    pub data: G,
}
pub struct CatalogMut<G> {
    pub data: G,
}

impl<G> PageAccessor for Catalog<G>
where
    G: Deref<Target = [u8; PAGE_SIZE]>,
{
    fn data(&self) -> &[u8; PAGE_SIZE] {
        &self.data
    }
}
impl<G> PageAccessor for CatalogMut<G>
where
    G: Deref<Target = [u8; PAGE_SIZE]>,
{
    fn data(&self) -> &[u8; PAGE_SIZE] {
        &self.data
    }
}
impl<G> PageAccessorMut for CatalogMut<G>
where
    G: DerefMut<Target = [u8; PAGE_SIZE]>,
{
    fn data_mut(&mut self) -> &mut [u8; PAGE_SIZE] {
        &mut self.data
    }
}
impl<G> CatalogMut<G> where G: DerefMut<Target = [u8; PAGE_SIZE]> {}

impl<G> SlottedPage for Catalog<G> where G: Deref<Target = [u8; PAGE_SIZE]> {}
impl<G> SlottedPage for CatalogMut<G> where G: Deref<Target = [u8; PAGE_SIZE]> {}
impl<G> SlottedPageMut for CatalogMut<G> where G: DerefMut<Target = [u8; PAGE_SIZE]> {}
