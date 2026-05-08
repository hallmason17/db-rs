use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicI32, AtomicUsize, Ordering},
    },
};

use parking_lot::{Mutex, RwLock};

use crate::{
    DbError, DbResult, FrameHandle, PAGE_SIZE, PageGuard, PageId, storage::StorageManager,
};

impl<'a> FrameHandle<'a> {
    pub fn new(frame: &'a Frame) -> Self {
        Self {
            page_id: frame.page_id,
            data: Arc::clone(&frame.data),
            frame,
        }
    }
}

impl<'a> Drop for FrameHandle<'a> {
    fn drop(&mut self) {
        self.frame.unpin();
    }
}

#[allow(dead_code)]
pub enum ReplacementStrategy {
    Fifo,
    Clock,
    Lru(u32),
    Lfu,
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct Frame {
    pub page_id: PageId,
    pub data: Arc<RwLock<[u8; PAGE_SIZE]>>,
    pin_count: AtomicI32,
    clock_flag: AtomicBool,
    dirty: AtomicBool,
}
impl Frame {
    pub fn mark_dirty(&self) -> () {
        self.dirty.fetch_or(true, Ordering::Relaxed);
    }
    pub fn unpin(&self) -> () {
        self.pin_count.fetch_sub(1, Ordering::Relaxed);
    }
}
impl Default for Frame {
    fn default() -> Self {
        let buf = [0u8; PAGE_SIZE];
        Self {
            page_id: PageId {
                file_id: 0,
                page_num: 0,
            },
            data: Arc::new(RwLock::new(buf)),
            pin_count: Default::default(),
            clock_flag: Default::default(),
            dirty: Default::default(),
        }
    }
}

type FrameNum = u64;

pub struct BufferPool {
    replacement_strategy: ReplacementStrategy,
    frames: Vec<Frame>,
    clock_hand: AtomicUsize,
    free_frames: Mutex<Vec<usize>>,
    page_to_frame_map: RwLock<HashMap<PageId, FrameNum>>,
    frame_to_page_map: RwLock<HashMap<FrameNum, PageId>>,
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
    ) -> Self {
        let mut frames = vec![];
        let mut free_frames = vec![];
        for i in 0..num_pages {
            frames.push(Frame::default());
            free_frames.push(i as usize);
        }
        Self {
            replacement_strategy,
            frames,
            clock_hand: AtomicUsize::new(0),
            free_frames: Mutex::new(free_frames),
            page_to_frame_map: RwLock::new(HashMap::new()),
            frame_to_page_map: RwLock::new(HashMap::new()),
            storage_mgr,
        }
    }
    pub fn flush_all(&self) -> anyhow::Result<()> {
        let mut storage = self.storage_mgr.write();
        for frame in &self.frames {
            if frame.dirty.load(Ordering::Acquire) {
                storage.write_block(&frame.page_id, &frame.data.read())?;
                frame.dirty.store(false, Ordering::Release);
            }
        }
        Ok(())
    }
    fn find_entry_in_map(&self, page_id: &PageId) -> Option<FrameTableEntry> {
        let map = self.page_to_frame_map.read();
        map.get_key_value(page_id).map(|entry| FrameTableEntry {
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
            if frame.pin_count.load(Ordering::Acquire) == 0 {
                if frame.clock_flag.load(Ordering::Relaxed) {
                    frame.clock_flag.store(false, Ordering::Relaxed);
                } else {
                    return Some(clock_hand as FrameNum);
                }
            }
        }
        None
    }
    fn select_victim(&self) -> Option<FrameNum> {
        if !self.free_frames.lock().is_empty() {
            self.free_frames.lock().pop().map(|x| x as FrameNum)
        } else {
            match self.replacement_strategy {
                ReplacementStrategy::Clock => self.select_victim_clock(),
                _ => None,
            }
        }
    }
    fn evict_page(&self, frame_num: FrameNum) -> DbResult<()> {
        let page_num = {
            let mut frame_map = self.frame_to_page_map.write();
            let mut page_map = self.page_to_frame_map.write();
            if let Some(page) = frame_map.remove(&frame_num) {
                page_map.remove(&page);
                Some(page)
            } else {
                None
            }
        };

        let frame = &self.frames[frame_num as usize];
        if let Some(page) = page_num
            && frame.dirty.load(Ordering::Relaxed)
        {
            let mut sm = self.storage_mgr.write();
            sm.write_block(&page, &frame.data.read())?;
        }

        Ok(())
    }
    pub fn get_page(&self, page_id: &PageId) -> DbResult<PageGuard<'_>> {
        // TODO: refactor, wrote the same thing multiple times. maybe change to let frame_index = if...
        // Check the map first
        if let Some(entry) = self.find_entry_in_map(page_id) {
            let frame = &self.frames[entry.frame as usize];
            frame.pin_count.fetch_add(1, Ordering::Relaxed);
            frame.clock_flag.fetch_or(true, Ordering::Relaxed);
            let handle = FrameHandle::new(frame);
            Ok(PageGuard { handle })
        } else if let Some(frame_index) = self.select_victim() {
            let frame = &self.frames[frame_index as usize];
            self.evict_page(frame_index as FrameNum)?;

            self.frame_to_page_map.write().insert(frame_index, *page_id);
            self.page_to_frame_map.write().insert(*page_id, frame_index);

            frame.clock_flag.store(true, Ordering::Relaxed);
            frame.pin_count.fetch_add(1, Ordering::Relaxed);

            let mut data = frame.data.write();
            self.storage_mgr.read().read_block(
                page_id,
                data.as_mut_array()
                    .expect("couldn't write to frame buffer!"),
            )?;
            let handle = FrameHandle::new(frame);
            Ok(PageGuard { handle })
        } else {
            Err(DbError::Unknown)
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
