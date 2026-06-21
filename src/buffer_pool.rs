/* Copyright (C) 2026 Mason Hall.
 *
 * GNU General Public License v3.0+ (see COPYING or https://www.gnu.org/licenses/gpl-3.0.txt)
 */
use std::{
    cell::{Ref, RefCell, RefMut},
    collections::HashMap,
};

use crate::{
    error::{Error, Result},
    ids::{FileId, PageId},
    page::{PAGE_SIZE, Page, PageAccessor, PageHeaderMut, PageHeaderReader, PageKind},
    storage::StorageManager,
};

#[derive(Debug)]
pub enum ReplacementStrategy {
    Clock,
}

#[derive(Debug, Clone)]
pub struct Frame {
    pub(crate) data: RefCell<[u8; PAGE_SIZE]>,
    pub state: RefCell<FrameState>,
}
#[derive(Debug, Clone)]
pub struct FrameState {
    pub page_id: Option<PageId>,
    pub pin_count: i32,
    pub clock_flag: bool,
    pub dirty: bool,
}
impl Frame {
    pub fn mark_dirty(&self) {
        self.state.borrow_mut().dirty = true;
    }
    pub fn unpin(&self) {
        let mut state = self.state.borrow_mut();
        state.pin_count -= 1;
        tracing::trace!("unpin page: {:?} -> {}", state.page_id, state.pin_count);
    }
    pub fn pin(&self) {
        let mut state = self.state.borrow_mut();
        state.pin_count += 1;
        state.clock_flag = true;
        tracing::trace!("pin page: {:?} -> {}", state.page_id, state.pin_count);
    }
}
impl Default for Frame {
    fn default() -> Self {
        let buf = RefCell::new([0u8; PAGE_SIZE]);
        Self {
            data: buf,
            state: RefCell::new(FrameState {
                page_id: None,
                pin_count: 0,
                clock_flag: false,
                dirty: false,
            }),
        }
    }
}

#[derive(Debug)]
pub struct PageGuard<'pg> {
    pub page_id: PageId,
    frame: &'pg Frame,
}
impl PageGuard<'_> {
    pub(crate) fn borrow_data(&self) -> Ref<'_, [u8; PAGE_SIZE]> {
        self.frame.data.borrow()
    }
    pub(crate) fn borrow_data_mut(&self) -> RefMut<'_, [u8; PAGE_SIZE]> {
        self.frame.mark_dirty();
        self.frame.data.borrow_mut()
    }
    pub fn kind(&self) -> PageKind {
        let page = Page {
            data: self.frame.data.borrow(),
        };
        page.header().kind()
    }
}
impl Drop for PageGuard<'_> {
    fn drop(&mut self) {
        self.frame.unpin();
    }
}

pub type FrameNum = u64;

#[derive(Debug)]
pub struct BufferPool {
    pub storage_manager: RefCell<StorageManager>,
    replacement_strategy: ReplacementStrategy,
    frames: Vec<Frame>,
    clock_hand: RefCell<usize>,
    free_frames: RefCell<Vec<usize>>,
    page_table: RefCell<HashMap<PageId, FrameNum>>,
}

impl BufferPool {
    pub fn new(
        num_pages: u64,
        replacement_strategy: ReplacementStrategy,
        storage_manager: StorageManager,
    ) -> Result<Self> {
        let frames = vec![Frame::default(); usize::try_from(num_pages)?];
        let free_frames = (0..num_pages)
            .map(usize::try_from)
            .collect::<core::result::Result<Vec<_>, _>>()?;
        Ok(Self {
            replacement_strategy,
            frames,
            clock_hand: 0.into(),
            free_frames: free_frames.into(),
            page_table: RefCell::new(HashMap::new()),
            storage_manager: RefCell::new(storage_manager),
        })
    }
    pub fn flush_all(&self) -> Result<()> {
        for frame in &self.frames {
            let state = frame.state.borrow();
            if state.dirty {
                self.storage_manager
                    .borrow_mut()
                    .write_block(&state.page_id.unwrap(), &frame.data.borrow())?;
            }
        }
        Ok(())
    }
    fn find_entry_in_map(&self, page_id: PageId) -> Option<FrameNum> {
        self.page_table.borrow().get(&page_id).copied()
    }
    fn select_victim_clock(&self) -> Option<FrameNum> {
        for _ in 0..self.frames.len() * 4 {
            let clock_hand = *self.clock_hand.borrow();
            *self.clock_hand.borrow_mut() = (clock_hand + 1) % self.frames.len();

            let frame = &self.frames[clock_hand];
            let mut state = frame.state.borrow_mut();
            if state.pin_count == 0 {
                if state.clock_flag {
                    state.clock_flag = false;
                } else {
                    return Some(clock_hand as FrameNum);
                }
            }
        }
        None
    }

    fn select_victim(&self) -> Option<FrameNum> {
        if let Some(frame) = self.free_frames.borrow_mut().pop() {
            Some(frame as FrameNum)
        } else {
            tracing::debug!("Selecting eviction victim!");
            match self.replacement_strategy {
                ReplacementStrategy::Clock => self.select_victim_clock(),
            }
        }
    }
    fn evict_page(&self, frame_num: FrameNum) -> Result<()> {
        let frame = &self.frames[usize::try_from(frame_num)?];
        tracing::debug!(
            "EVICTING FRAME: {}, (page {:?})",
            frame_num,
            frame.state.borrow().page_id
        );
        let state = &mut frame.state.borrow_mut();
        assert!(state.pin_count == 0);
        let page_num = state.page_id;
        if let Some(page) = page_num {
            self.page_table.borrow_mut().remove(&page);
            if state.dirty {
                self.storage_manager
                    .borrow_mut()
                    .write_block(&page, &frame.data.borrow())?;
            }
        }
        state.dirty = false;
        state.clock_flag = false;
        state.page_id = None;

        Ok(())
    }

    pub fn get_page(&self, page_id: PageId) -> Result<PageGuard<'_>> {
        if let Some(frame_num) = self.find_entry_in_map(page_id) {
            tracing::debug!("Cache hit for page {:?} in frame {}", page_id, frame_num);
            let frame = &self.frames[usize::try_from(frame_num)?];
            frame.pin();
            Ok(PageGuard { frame, page_id })
        } else if let Some(frame_index) = self.select_victim() {
            tracing::debug!("Cache miss for page {:?}, loading from disk", page_id);
            self.evict_page(frame_index as FrameNum)?;
            let frame = &self.frames[usize::try_from(frame_index)?];
            {
                let mut state = frame.state.borrow_mut();

                state.page_id = Some(PageId {
                    file_id: page_id.file_id,
                    page_num: page_id.page_num,
                });
                state.pin_count = 0;
                state.clock_flag = false;
                state.dirty = false;
            }
            assert!(!self.page_table.borrow().contains_key(&page_id));
            self.page_table.borrow_mut().insert(page_id, frame_index);
            tracing::trace!("INSERT PAGE {:?} INTO FRAME {}", page_id, frame_index);
            frame.pin();

            // If this fails we need to unpin!!!!!!!!! Hours of debugging later...
            if let Err(e) = self
                .storage_manager
                .borrow_mut()
                .read_block(&page_id, &mut frame.data.borrow_mut())
            {
                frame.unpin();
                self.page_table.borrow_mut().remove(&page_id);
                frame.state.borrow_mut().page_id = None;
                return Err(e);
            }

            Ok(PageGuard { frame, page_id })
        } else {
            Err(Error::NoPagesAvailable)
        }
    }

    pub fn create_page(&self, file_id: FileId, kind: PageKind) -> Result<PageGuard<'_>> {
        if let Some(frame_index) = self.select_victim() {
            let page_num = self
                .storage_manager
                .borrow_mut()
                .get_next_page_id(file_id)?;
            tracing::debug!("Creating new page ID {} of kind {:?}", page_num, kind);
            tracing::debug!("next page id: {}", page_num);
            self.evict_page(frame_index as FrameNum)?;

            let frame = &self.frames[usize::try_from(frame_index)?];
            self.page_table
                .borrow_mut()
                .insert(PageId { file_id, page_num }, frame_index);

            {
                let mut state = frame.state.borrow_mut();
                state.dirty = true;
                state.page_id = Some(PageId { file_id, page_num });
            }
            frame.pin();

            {
                let mut data = frame.data.borrow_mut();
                let mut header = PageHeaderMut::new(&mut data[..]);
                header.set_kind(kind);
                header.set_num_entries(0);
                header.set_page_id(page_num);
            }

            Ok(PageGuard {
                frame,
                page_id: PageId { file_id, page_num },
            })
        } else {
            tracing::error!(
                "couldnt create page, {:?} \n PageTable: {:?}",
                self.frames
                    .iter()
                    .map(|f| f.state.borrow())
                    .collect::<Vec<_>>(),
                self.page_table
            );
            Err(Error::Unknown)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::page::PageKind;
    use tempfile::tempdir;

    fn setup(frame_count: u64) -> (tempfile::TempDir, BufferPool) {
        let dir = tempdir().unwrap();
        let sm = StorageManager::new(dir.path()).unwrap();
        let bp = BufferPool::new(frame_count, ReplacementStrategy::Clock, sm).unwrap();
        (dir, bp)
    }

    #[test]
    fn initial_state_has_free_frames() {
        let (_dir, bp) = setup(4);
        assert_eq!(bp.frames.len(), 4);
        assert_eq!(bp.free_frames.borrow().len(), 4);
        assert!(bp.page_table.borrow().is_empty());
    }

    #[test]
    fn create_page_pins_and_appears_in_page_table() {
        let (_dir, bp) = setup(4);
        let pnum = {
            let guard = bp.create_page(FileId(0), PageKind::Heap).unwrap();
            guard.page_id
        };
        assert_eq!(bp.page_table.borrow().len(), 1);
        assert!(bp.page_table.borrow().contains_key(&pnum));
    }

    #[test]
    fn create_page_reuses_evicted_frame() {
        let (_dir, bp) = setup(2);

        let (p1, p2) = {
            let g1 = bp.create_page(FileId(0), PageKind::Heap).unwrap();
            let p1 = g1.page_id;
            drop(g1);
            let g2 = bp.create_page(FileId(0), PageKind::Heap).unwrap();
            let p2 = g2.page_id;
            drop(g2);
            (p1, p2)
        };

        assert_eq!(
            bp.free_frames.borrow().len(),
            0,
            "free_frames should be empty"
        );

        let p3 = {
            let g3 = bp.create_page(FileId(0), PageKind::Heap).unwrap();
            g3.page_id
        };

        assert_eq!(
            bp.page_table.borrow().len(),
            2,
            "table should hold at most 2 entries"
        );
        assert!(bp.page_table.borrow().contains_key(&p3));
        let evicted =
            !bp.page_table.borrow().contains_key(&p1) || !bp.page_table.borrow().contains_key(&p2);
        assert!(
            evicted,
            "expected p1 or p2 to be evicted, table has: {:?}",
            bp.page_table
        );
    }

    // TODO: Need refactor to test this (BufferPool mut)...
    #[test]
    fn multiple_pins_same_page_via_cache_hit() {
        let (_dir, bp) = setup(4);
        let p1 = {
            let g = bp.create_page(FileId(0), PageKind::Heap).unwrap();
            g.page_id
        };

        // First get_page — cache miss, loads from disk.
        let g = bp.get_page(p1).unwrap();
        {
            let state = g.frame.state.borrow();
            assert_eq!(state.pin_count, 1);
        }

        // Second get_page — cache hit, pin_count increments.
        let g = bp.get_page(p1).unwrap();
        {
            let state = g.frame.state.borrow();
            assert_eq!(state.pin_count, 2);
        }
    }

    #[test]
    fn get_page_cache_hit_returns_same_data() {
        let (_dir, bp) = setup(4);

        let p1 = {
            let guard = bp.create_page(FileId(0), PageKind::Heap).unwrap();
            let p1 = guard.page_id;
            guard.frame.data.borrow_mut()[0..4].copy_from_slice(&[0xCA, 0xFE, 0xBA, 0xBE]);
            guard.frame.mark_dirty();
            p1
        };

        bp.flush_all().unwrap();

        {
            let guard = bp.get_page(p1).unwrap();
            assert_eq!(guard.frame.data.borrow()[0..4], [0xCA, 0xFE, 0xBA, 0xBE]);
        }
        {
            let guard = bp.get_page(p1).unwrap();
            assert_eq!(guard.frame.data.borrow()[0..4], [0xCA, 0xFE, 0xBA, 0xBE]);
        }
    }
}
