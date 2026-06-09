use std::{
    cell::{Ref, RefMut},
    ops::{Deref, DerefMut},
};

use crate::{
    PageGuard,
    error::Result,
    page::{PAGE_SIZE, PageAccessor, PageAccessorMut, SlottedPage, SlottedPageMut},
};

use super::PageKind;

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

impl PageGuard<'_> {
    pub fn as_catalog(&self) -> Result<Catalog<Ref<'_, [u8; PAGE_SIZE]>>> {
        let page = self.cast_read(PageKind::Catalog)?;
        Ok(Catalog { data: page.data })
    }

    pub fn as_catalog_mut(&mut self) -> Result<CatalogMut<RefMut<'_, [u8; PAGE_SIZE]>>> {
        let page = self.cast_write(PageKind::Catalog)?;
        Ok(CatalogMut { data: page.data })
    }
}
