use std::{
    cell::{Ref, RefMut},
    ops::{Deref, DerefMut},
};

use crate::{
    PageGuard,
    error::DbResult,
    page::{PAGE_SIZE, PageAccessor, PageAccessorMut, page_header_offsets},
};

use super::PageKind;

pub struct FreeSpaceMap<G> {
    pub data: G,
}

pub struct FreeSpaceMapMut<G> {
    pub data: G,
}

impl<G> FreeSpaceMapper for FreeSpaceMap<G> where G: Deref<Target = [u8; PAGE_SIZE]> {}

impl<G> FreeSpaceMapper for FreeSpaceMapMut<G> where G: Deref<Target = [u8; PAGE_SIZE]> {}

impl<G> FreeSpaceMapperMut for FreeSpaceMapMut<G> where G: DerefMut<Target = [u8; PAGE_SIZE]> {}

impl<G> FreeSpaceMapMut<G> where G: DerefMut<Target = [u8; PAGE_SIZE]> {}

impl<G> PageAccessor for FreeSpaceMap<G>
where
    G: Deref<Target = [u8; PAGE_SIZE]>,
{
    fn data(&self) -> &[u8; PAGE_SIZE] {
        &self.data
    }
}

impl<G> PageAccessor for FreeSpaceMapMut<G>
where
    G: Deref<Target = [u8; PAGE_SIZE]>,
{
    fn data(&self) -> &[u8; PAGE_SIZE] {
        &self.data
    }
}

impl<G> PageAccessorMut for FreeSpaceMapMut<G>
where
    G: DerefMut<Target = [u8; PAGE_SIZE]>,
{
    fn data_mut(&mut self) -> &mut [u8; PAGE_SIZE] {
        &mut self.data
    }
}

pub trait FreeSpaceMapper: PageAccessor {
    fn fsm_num(&self) -> u16 {
        u16::from_be_bytes(
            self.data()[page_header_offsets::fsm_page::FSM_NUM
                ..page_header_offsets::fsm_page::FSM_NUM + 2]
                .try_into()
                .unwrap(),
        )
    }
    fn find_first_free_page(&self, last_page_used: u64) -> u64 {
        let max = self.max_pages();
        let start = last_page_used % max;
        for offset in 0..max {
            let i = (start + offset) % max;
            if i < 2 && self.fsm_num() == 0 {
                continue;
            }
            if !self.is_page_full(i) {
                let num = self.fsm_num();
                return i + (num as u64 * max);
            }
        }
        u64::MAX
    }
    fn is_page_full(&self, page_id: u64) -> bool {
        let page_id = page_id % self.max_pages();
        let byte = (page_id / 8) as usize + page_header_offsets::fsm_page::SIZE;

        let shamt = (7 - (page_id % 8)) as u8;

        self.data()[byte] & (1u8 << shamt) == 0
    }
    fn max_pages(&self) -> u64 {
        ((PAGE_SIZE - page_header_offsets::fsm_page::SIZE) * 8) as u64
    }
    fn is_full(&self) -> bool {
        self.is_page_full(self.max_pages() - 1)
    }
}
pub trait FreeSpaceMapperMut: FreeSpaceMapper + PageAccessorMut {
    fn set_page_full(&mut self, page_id: u64) {
        let page_id = page_id % self.max_pages();
        // Ex. page_id: 15
        //
        // Byte: 1
        // Bit offset (from right): (7 - 7) = 0
        //                                  => shifted: 00000001
        // [11111111, 10110111, ...]
        //        & !(00000001)
        //
        //            10110111
        //          & 11111110
        //          = 10110110
        //
        // [11111111, 10110110, ...]
        let byte = (page_id / 8) as usize + page_header_offsets::fsm_page::SIZE;

        let shamt = (7 - (page_id % 8)) as u8;

        self.data_mut()[byte] &= !(1u8 << shamt);
    }

    fn set_fsm_num(&mut self, fsm_num: u16) {
        self.data_mut()
            [page_header_offsets::fsm_page::FSM_NUM..page_header_offsets::fsm_page::FSM_NUM + 2]
            .copy_from_slice(&fsm_num.to_be_bytes());
    }
}

impl PageGuard<'_> {
    pub fn as_fsm(&self) -> DbResult<FreeSpaceMap<Ref<'_, [u8; PAGE_SIZE]>>> {
        let page = self.cast_read(PageKind::FreeSpaceMap)?;
        Ok(FreeSpaceMap { data: page.data })
    }

    pub fn as_fsm_mut(&mut self) -> DbResult<FreeSpaceMapMut<RefMut<'_, [u8; PAGE_SIZE]>>> {
        let page = self.cast_write(PageKind::FreeSpaceMap)?;
        Ok(FreeSpaceMapMut { data: page.data })
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    struct MockFsmPage {
        data: [u8; PAGE_SIZE],
    }

    impl MockFsmPage {
        fn new(fsm_num: u16) -> Self {
            let mut data = [0xFF; PAGE_SIZE];

            data[page_header_offsets::fsm_page::FSM_NUM
                ..page_header_offsets::fsm_page::FSM_NUM + 2]
                .copy_from_slice(&fsm_num.to_be_bytes());

            Self { data }
        }
    }

    impl PageAccessor for MockFsmPage {
        fn data(&self) -> &[u8; PAGE_SIZE] {
            &self.data
        }
    }
    impl PageAccessorMut for MockFsmPage {
        fn data_mut(&mut self) -> &mut [u8; PAGE_SIZE] {
            &mut self.data
        }
    }

    impl FreeSpaceMapper for MockFsmPage {}
    impl FreeSpaceMapperMut for MockFsmPage {}

    #[test]
    fn test_initialization_and_metadata() {
        let fsm = MockFsmPage::new(42);
        assert_eq!(fsm.fsm_num(), 42);

        let expected_max = ((PAGE_SIZE - page_header_offsets::fsm_page::SIZE) * 8) as u64;
        assert_eq!(fsm.max_pages(), expected_max);

        assert!(!fsm.is_full());
    }

    #[test]
    fn test_single_page_bit_manipulation() {
        let mut fsm = MockFsmPage::new(0);

        let target_page = 5;

        assert!(!fsm.is_page_full(target_page));

        fsm.set_page_full(target_page);

        assert!(fsm.is_page_full(target_page));
        assert!(!fsm.is_page_full(target_page - 1));
    }

    #[test]
    fn test_first_free_page_linear_progression() {
        let mut fsm = MockFsmPage::new(0);

        fsm.set_page_full(2);
        fsm.set_page_full(3);
        fsm.set_page_full(4);

        assert_eq!(fsm.find_first_free_page(2), 5);
    }

    #[test]
    fn test_fsm_chunk_scaling_math() {
        let mut fsm = MockFsmPage::new(2);
        let max_pages_per_chunk = fsm.max_pages();

        fsm.set_page_full(2);

        let expected_global_id = 3 + (2 * max_pages_per_chunk);

        assert_eq!(fsm.find_first_free_page(2), expected_global_id);
    }

    #[test]
    fn test_global_id_modulo_translation() {
        let mut fsm = MockFsmPage::new(1);
        let max_pages_per_chunk = fsm.max_pages();

        let deep_global_page_id = max_pages_per_chunk + 10;

        fsm.set_page_full(deep_global_page_id);

        assert!(fsm.is_page_full(deep_global_page_id));
        assert!(fsm.is_page_full(10));

        assert!(!fsm.is_page_full(9));
    }

    #[test]
    fn test_is_full_boundary() {
        let mut fsm = MockFsmPage::new(0);
        let last_bit_index = fsm.max_pages() - 1;

        assert!(!fsm.is_full());

        fsm.set_page_full(last_bit_index);

        assert!(fsm.is_full());
    }
}
