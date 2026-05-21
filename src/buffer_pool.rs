use std::collections::HashMap;

use crate::{
    DbError, Frame, PageGuard, PageId,
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
    ) -> anyhow::Result<Self> {
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
    pub fn flush_all(&mut self) -> anyhow::Result<()> {
        for frame in &mut self.frames {
            if frame.state.dirty {
                self.storage_manager
                    .write_block(&frame.state.page_id.unwrap(), &frame.data.borrow())?;
            }
        }
        Ok(())
    }
    fn find_entry_in_map(&self, page_id: PageId) -> Option<FrameTableEntry> {
        self.page_table
            .get_key_value(&page_id)
            .map(|entry| FrameTableEntry {
                page: *entry.0,
                frame: *entry.1,
            })
    }
    fn select_victim_clock(&mut self) -> Option<FrameNum> {
        for _ in 0..self.frames.len() * 4 {
            let clock_hand = self.clock_hand;
            self.clock_hand = (clock_hand + 1) % self.frames.len();

            let frame = &mut self.frames[clock_hand];
            if frame.state.pin_count == 0 {
                if frame.state.clock_flag {
                    frame.state.clock_flag = false;
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
    fn evict_page(&mut self, frame_num: FrameNum) -> anyhow::Result<()> {
        let frame = &self.frames[usize::try_from(frame_num)?];
        let state = &frame.state;
        let page_num = state.page_id;
        if let Some(page) = page_num {
            self.page_table.remove(&page);
            if state.dirty {
                let write_buf = frame.data.clone();
                self.storage_manager
                    .write_block(&page, &write_buf.borrow())?;
            }
        }

        Ok(())
    }

    pub fn get_page(&mut self, page_id: PageId) -> anyhow::Result<PageGuard> {
        if let Some(frame_num) = self.find_entry_in_map(page_id) {
            let frame = &mut self.frames[usize::try_from(frame_num.frame)?];
            frame.state.pin_count += 1;
            frame.state.clock_flag = true;
            Ok(PageGuard {
                frame: &self.frames[usize::try_from(frame_num.frame)?],
                page_id,
            })
        } else if let Some(frame_index) = self.select_victim() {
            self.page_table.insert(page_id, frame_index);
            self.evict_page(frame_index as FrameNum)?;
            let frame = &mut self.frames[usize::try_from(frame_index)?];

            frame.state.clock_flag = true;
            frame.state.pin_count += 1;
            frame.state.page_id = Some(PageId {
                file_id: page_id.file_id,
                page_num: page_id.page_num,
            });

            self.storage_manager
                .read_block(&page_id, &mut frame.data.borrow_mut())?;
            Ok(PageGuard {
                frame: &self.frames[usize::try_from(frame_index)?],
                page_id,
            })
        } else {
            Err(DbError::NoPagesAvailable.into())
        }
    }

    pub fn create_page(&mut self, file_id: u32, kind: PageKind) -> anyhow::Result<PageGuard> {
        if let Some(frame_index) = self.select_victim() {
            let page_num = self.storage_manager.get_next_page_id(file_id)?;
            tracing::debug!("next page id: {}", page_num);
            self.evict_page(frame_index as FrameNum)?;

            let frame = &mut self.frames[usize::try_from(frame_index)?];

            self.page_table
                .insert(PageId { file_id, page_num }, frame_index);

            {
                frame.state.dirty = true;
                frame.state.clock_flag = true;
                frame.state.pin_count += 1;
                frame.state.page_id = Some(PageId { file_id, page_num });
            }

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
            Err(DbError::Unknown.into())
        }
    }
}

#[cfg(test)]
mod tests {

    /*
       fn setup_bp(num_pages: u64) -> BufferPool {
           let dir = tempdir().unwrap();
           let sm = Arc::new(RwLock::new(
               StorageManager::new(std::env::current_dir().unwrap().as_path()).unwrap(),
           ));
           sm.write().ensure_capacity(0, 31).unwrap();
           BufferPool::new(num_pages, ReplacementStrategy::Clock, sm);
       }

       #[test]
       fn test_buffer_pool_initialization() {
           let bp = setup_bp(3);
           assert_eq!(bp.free_frames.lock().len(), 3);
           assert_eq!(bp.frames.len(), 3);
       }

       #[test]
       fn test_pin_and_unpin_logic() {
           let bp = setup_bp(3);

           {
               let handle = bp
                   .get_page(&PageId {
                       file_id: 0,
                       page_num: 0,
                   })
                   .expect("Should pin page 0");
               assert_eq!(handle.frame.pin_count.load(Ordering::Relaxed), 1);
               assert!(
                   bp.page_to_frame_map
                       .read()
                       .get(&PageId {
                           file_id: 0,
                           page_num: 0
                       })
                       .is_some()
               );

               let handle2 = bp
                   .get_page(&PageId {
                       file_id: 0,
                       page_num: 0,
                   })
                   .expect("Should pin page 0 again");
               assert!(handle2.frame.pin_count.load(Ordering::Relaxed) >= 1);
               assert!(
                   bp.page_to_frame_map
                       .read()
                       .get(&PageId {
                           file_id: 0,
                           page_num: 0
                       })
                       .is_some()
               );
           }

           let frame_idx = *bp
               .page_to_frame_map
               .read()
               .get(&PageId {
                   file_id: 0,
                   page_num: 0,
               })
               .unwrap();
           assert_eq!(
               bp.frames[frame_idx as usize]
                   .pin_count
                   .load(Ordering::Relaxed),
               0
           );
       }

       #[test]
       fn test_clock_eviction_strategy() {
           let bp = setup_bp(2);

           {
               let _h1 = bp
                   .get_page(&PageId {
                       file_id: 0,
                       page_num: 10,
                   })
                   .unwrap();
               let _h2 = bp
                   .get_page(&PageId {
                       file_id: 0,
                       page_num: 20,
                   })
                   .unwrap();
           }
           bp.get_page(&PageId {
               file_id: 0,
               page_num: 30,
           })
           .expect("Should evict a page to make room for page 30");

           let map = bp.page_to_frame_map.read();
           println!("{:?}", map);
           assert!(map.keys().any(|&v| v.page_num == 30));
           assert_eq!(map.len(), 2);
       }

       #[test]
       fn test_all_pages_pinned_error() {
           let bp = setup_bp(2);

           let _h1 = bp
               .get_page(&PageId {
                   file_id: 0,
                   page_num: 1,
               })
               .unwrap();
           let _h2 = bp
               .get_page(&PageId {
                   file_id: 0,
                   page_num: 2,
               })
               .unwrap();

           let result = bp.get_page(&PageId {
               file_id: 0,
               page_num: 3,
           });
           assert!(result.is_err(), "Should fail when no victims are available");
       }
    */
}
