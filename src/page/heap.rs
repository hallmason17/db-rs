/* Copyright (C) 2026 Mason Hall.
 *
 * This file is part of db-rs.
 *
 * db-rs is free software: you can redistribute it and/or modify it under the
 * terms of the GNU General Public License as published by the Free Software
 * Foundation, either version 3 of the License, or (at your option) any later
 * version.
 *
 * db-rs is distributed in the hope that it will be useful, but WITHOUT ANY
 * WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS
 * FOR A PARTICULAR PURPOSE. See the GNU General Public License for more
 * details.
 *
 * You should have received a copy of the GNU General Public License along with
 * db-rs. If not, see <https://www.gnu.org/licenses/>.
 */
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
    pub fn with_heap_mut<T>(
        &mut self,
        f: impl FnOnce(&mut HeapMut<RefMut<'_, [u8; PAGE_SIZE]>>) -> Result<T>,
    ) -> Result<T> {
        let page = self.cast_write(PageKind::Heap)?;
        f(&mut HeapMut { data: page.data })
    }
    pub fn with_heap<T>(
        &self,
        f: impl FnOnce(&Heap<Ref<'_, [u8; PAGE_SIZE]>>) -> Result<T>,
    ) -> Result<T> {
        let page = self.cast_read(PageKind::Heap)?;
        f(&Heap { data: page.data })
    }
    pub fn as_heap(&self) -> Result<Heap<Ref<'_, [u8; PAGE_SIZE]>>> {
        let page = self.cast_read(PageKind::Heap)?;
        Ok(Heap { data: page.data })
    }
    pub fn as_heap_mut(&mut self) -> Result<HeapMut<RefMut<'_, [u8; PAGE_SIZE]>>> {
        let page = self.cast_write(PageKind::Heap)?;
        Ok(HeapMut { data: page.data })
    }
}
