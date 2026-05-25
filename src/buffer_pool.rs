use std::collections::HashMap;

use crate::{
    Frame, PageGuard, PageId,
    error::{DbError, DbResult},
    page::{PageHeaderMut, PageKind},
    page_header_offsets,
    storage::StorageManager,
};

#[allow(dead_code)]
#[derive(Debug)]
pub enum ReplacementStrategy {
    Fifo,
    Clock,
    Lru(u32),
    Lfu,
}

pub type FrameNum = u64;

#[derive(Debug)]
pub struct BufferPool {
    pub storage_manager: StorageManager,
    replacement_strategy: ReplacementStrategy,
    frames: Vec<Frame>,
    clock_hand: usize,
    free_frames: Vec<usize>,
    page_table: HashMap<PageId, FrameNum>,
}

struct FrameTableEntry {
    frame: u64,
    page: PageId,
}

impl BufferPool {
    pub fn new(
        num_pages: u64,
        replacement_strategy: ReplacementStrategy,
        storage_manager: StorageManager,
    ) -> DbResult<Self> {
        let mut frames = vec![];
        let mut free_frames = vec![];
        for i in 0..num_pages {
            frames.push(Frame::default());
            free_frames.push(usize::try_from(i)?);
        }
        Ok(Self {
            replacement_strategy,
            frames,
            clock_hand: 0,
            free_frames,
            page_table: HashMap::new(),
            storage_manager,
        })
    }
    pub fn flush_all(&mut self) -> DbResult<()> {
        for frame in &mut self.frames {
            let state = frame.state.borrow();
            if state.dirty {
                self.storage_manager
                    .write_block(&state.page_id.unwrap(), &frame.data.borrow())?;
            }
        }
        Ok(())
    }
    fn find_entry_in_map(&self, page_id: PageId) -> Option<FrameNum> {
        self.page_table.get(&page_id).copied()
    }
    fn select_victim_clock(&mut self) -> Option<FrameNum> {
        for _ in 0..self.frames.len() * 4 {
            let clock_hand = self.clock_hand;
            self.clock_hand = (clock_hand + 1) % self.frames.len();

            let frame = &mut self.frames[clock_hand];
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

    // TODO: First look to evict dirty pages.
    fn select_victim(&mut self) -> Option<FrameNum> {
        if let Some(frame) = self.free_frames.pop() {
            Some(frame as FrameNum)
        } else {
            match self.replacement_strategy {
                ReplacementStrategy::Clock => self.select_victim_clock(),
                _ => None,
            }
        }
    }
    fn evict_page(&mut self, frame_num: FrameNum) -> DbResult<()> {
        let frame = &mut self.frames[usize::try_from(frame_num)?];
        tracing::warn!("EVICTING FRAME: {}, {:?}", frame_num, frame.state.borrow());
        let state = &mut frame.state.borrow_mut();
        assert!(state.pin_count == 0);
        let page_num = state.page_id;
        if let Some(page) = page_num {
            self.page_table.remove(&page);
            if state.dirty {
                self.storage_manager
                    .write_block(&page, &frame.data.borrow())?;
            }
        }
        state.dirty = false;
        state.clock_flag = false;
        state.page_id = None;

        Ok(())
    }

    pub fn get_page(&mut self, page_id: PageId) -> DbResult<PageGuard<'_>> {
        if let Some(frame_num) = self.find_entry_in_map(page_id) {
            tracing::trace!("HIT {:?} PAGE TABLE {:?}", page_id, self.page_table);
            let frame = &self.frames[usize::try_from(frame_num)?];
            frame.pin();
            Ok(PageGuard { frame, page_id })
        } else if let Some(frame_index) = self.select_victim() {
            tracing::warn!("MISS {:?} PAGE TABLE {:?}", page_id, self.page_table);
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
            assert!(!self.page_table.contains_key(&page_id));
            self.page_table.insert(page_id, frame_index);
            tracing::trace!("INSERT PAGE {:?} INTO FRAME {}", page_id, frame_index);
            frame.pin();

            // If this fails we need to unpin!!!!!!!!! Hours of debugging later...
            if let Err(e) = self
                .storage_manager
                .read_block(&page_id, &mut frame.data.borrow_mut())
            {
                frame.unpin();
                self.page_table.remove(&page_id);
                frame.state.borrow_mut().page_id = None;
                return Err(e);
            }

            Ok(PageGuard { frame, page_id })
        } else {
            Err(DbError::NoPagesAvailable)
        }
    }

    pub fn create_page(&mut self, file_id: u32, kind: PageKind) -> DbResult<PageGuard<'_>> {
        if let Some(frame_index) = self.select_victim() {
            let page_num = self.storage_manager.get_next_page_id(file_id)?;
            tracing::debug!("next page id: {}", page_num);
            self.evict_page(frame_index as FrameNum)?;

            let frame = &mut self.frames[usize::try_from(frame_index)?];
            self.page_table
                .insert(PageId { file_id, page_num }, frame_index);

            {
                let mut state = frame.state.borrow_mut();
                state.dirty = true;
                state.page_id = Some(PageId { file_id, page_num });
            }
            frame.pin();

            {
                let mut data = frame.data.borrow_mut();
                if matches!(kind, PageKind::FreeSpaceMap) {
                    data.fill(u8::MAX);
                    data[page_header_offsets::fsm_page::FSM_NUM
                        ..page_header_offsets::fsm_page::FSM_NUM + 2]
                        .fill(0);
                }
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
            Err(DbError::Unknown)
        }
    }
}

#[cfg(test)]
mod tests {}
