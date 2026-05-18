use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicI32, AtomicUsize, Ordering},
    },
};

use parking_lot::{Mutex, RwLock};

use crate::{
    DbError, FrameHandle, PAGE_SIZE, PageGuard, PageId,
    page::{PageHeaderMut, PageKind},
    page_header_offsets,
    storage::StorageManager,
};

impl<'a> FrameHandle<'a> {
    pub fn new(frame: &'a Frame, page_id: PageId) -> Self {
        Self {
            page_id,
            data: Arc::clone(&frame.data),
            frame,
        }
    }
}

impl Drop for FrameHandle<'_> {
    fn drop(&mut self) {
        self.frame.unpin();
    }
}

#[allow(dead_code)]
#[derive(Debug)]
pub enum ReplacementStrategy {
    Fifo,
    Clock,
    Lru(u32),
    Lfu,
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct Frame {
    pub data: Arc<RwLock<[u8; PAGE_SIZE]>>,
    pub state: Mutex<FrameState>,
}
#[derive(Debug)]
pub struct FrameState {
    pub page_id: Option<PageId>,
    pub pin_count: i32,
    pub clock_flag: bool,
    pub dirty: bool,
}
impl Frame {
    pub fn mark_dirty(&self) {
        self.state.lock().dirty = true;
    }
    pub fn unpin(&self) {
        self.state.lock().pin_count += 1;
    }
}
impl Default for Frame {
    fn default() -> Self {
        let buf = [0u8; PAGE_SIZE];
        Self {
            data: Arc::new(RwLock::new(buf)),
            state: Mutex::new(FrameState {
                page_id: None,
                pin_count: 0,
                clock_flag: false,
                dirty: false,
            }),
        }
    }
}

type FrameNum = u64;

#[derive(Debug)]
pub struct BufferPool {
    replacement_strategy: ReplacementStrategy,
    frames: Vec<Frame>,
    clock_hand: AtomicUsize,
    free_frames: Mutex<Vec<usize>>,
    page_table: RwLock<HashMap<PageId, FrameNum>>,
    storage_mgr: Arc<RwLock<StorageManager>>,
}

struct FrameTableEntry {
    frame: u64,
    page: PageId,
}

impl BufferPool {
    pub fn new(
        num_pages: u64,
        replacement_strategy: ReplacementStrategy,
        storage_mgr: Arc<RwLock<StorageManager>>,
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
            clock_hand: AtomicUsize::new(0),
            free_frames: Mutex::new(free_frames),
            page_table: RwLock::new(HashMap::new()),
            storage_mgr,
        })
    }
    pub fn flush_all(&self) -> anyhow::Result<()> {
        let mut storage = self.storage_mgr.write();
        for frame in &self.frames {
            let mut state = frame.state.lock();
            if state.dirty {
                storage.write_block(&state.page_id.unwrap(), &frame.data.read())?;
                state.dirty = false;
            }
        }
        Ok(())
    }
    fn find_entry_in_map(&self, page_id: PageId) -> Option<FrameTableEntry> {
        let map = self.page_table.read();
        map.get_key_value(&page_id).map(|entry| FrameTableEntry {
            page: *entry.0,
            frame: *entry.1,
        })
    }
    fn select_victim_clock(&self) -> Option<FrameNum> {
        for _ in 0..self.frames.len() * 4 {
            let clock_hand = self.clock_hand.load(Ordering::Relaxed);
            let new_clock_hand = (clock_hand + 1) % self.frames.len();
            if self
                .clock_hand
                .compare_exchange_weak(
                    clock_hand,
                    new_clock_hand,
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                )
                .is_err()
            {
                continue;
            }

            let frame = &self.frames[clock_hand];
            let mut state = frame.state.lock();
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
    fn select_victim(&self) -> Option<FrameNum> {
        if self.free_frames.lock().is_empty() {
            match self.replacement_strategy {
                ReplacementStrategy::Clock => self.select_victim_clock(),
                _ => None,
            }
        } else {
            self.free_frames.lock().pop().map(|x| x as FrameNum)
        }
    }
    fn evict_page(&self, frame_num: FrameNum) -> anyhow::Result<()> {
        let frame = &self.frames[usize::try_from(frame_num)?];
        let state = frame.state.lock();
        let page_num = state.page_id;
        if let Some(page) = page_num
            && state.dirty
        {
            self.page_table.write().remove(&page);

            let write_buf = frame.data.read().clone();
            let mut sm = self.storage_mgr.write();
            sm.write_block(&page, &write_buf)?;
        }

        Ok(())
    }
    fn load_page(&self, page_id: PageId) -> anyhow::Result<u64> {
        Ok(0)
    }
    pub fn get_page(&self, page_id: PageId) -> anyhow::Result<PageGuard<'_>> {
        let pt = self.page_table.read();
        if let Some(&frame_num) = pt.get(&page_id) {
            let frame = &self.frames[usize::try_from(frame_num)?];
            let mut state = frame.state.lock();
            state.pin_count += 1;
            state.clock_flag = true;
            let handle = FrameHandle::new(frame, page_id);
            Ok(PageGuard { handle })
        } else if let Some(frame_index) = self.select_victim() {
            let frame = &self.frames[usize::try_from(frame_index)?];
            self.evict_page(frame_index as FrameNum)?;
            let mut state = frame.state.lock();

            state.clock_flag = true;
            state.pin_count += 1;
            state.page_id = Some(PageId {
                file_id: page_id.file_id,
                page_num: page_id.page_num,
            });

            let mut data = frame.data.write();
            self.storage_mgr.read().read_block(
                &page_id,
                data.as_mut_array()
                    .expect("couldn't write to frame buffer!"),
            )?;
            let handle = FrameHandle::new(frame, page_id);
            Ok(PageGuard { handle })
        } else {
            Err(DbError::Unknown.into())
        }
    }

    pub fn create_page(&self, file_id: u32, kind: PageKind) -> anyhow::Result<PageGuard<'_>> {
        if let Some(frame_index) = self.select_victim() {
            let page_num = self.storage_mgr.write().get_next_page_id(file_id)?;
            tracing::debug!("next page id: {}", page_num);
            self.evict_page(frame_index as FrameNum)?;

            let frame = &self.frames[usize::try_from(frame_index)?];

            self.page_table
                .write()
                .insert(PageId { file_id, page_num }, frame_index);

            {
                let mut state = frame.state.lock();
                state.dirty = true;
                state.clock_flag = true;
                state.pin_count += 1;
                state.page_id = Some(PageId { file_id, page_num });
            }

            {
                let mut data = frame.data.write();
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
                handle: FrameHandle::new(frame, PageId{file_id,page_num}),
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
