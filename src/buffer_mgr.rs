use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        Arc, RwLock,
        atomic::{AtomicBool, AtomicI32, Ordering},
    },
};

use crate::{DbError, DbResult, PAGE_SIZE, storage_mgr::StorageManager};

#[allow(dead_code)]
pub struct FrameHandle<'a> {
    frame: &'a Frame,
}

impl<'a> Drop for FrameHandle<'a> {
    fn drop(&mut self) {
        self.frame.pin_count.fetch_sub(1, Ordering::Relaxed);
    }
}

pub enum ReplacementStrategy {
    Fifo,
    Clock,
    Lru(u32),
    Lfu,
}

pub struct Frame {
    pub page_number: u64,
    pub data: Arc<RwLock<[u8; PAGE_SIZE]>>,
    pin_count: AtomicI32,
    clock_flag: AtomicBool,
    dirty: AtomicBool,
}
impl Frame {
    pub fn mark_dirty(&mut self) -> DbResult<()> {
        self.dirty.fetch_or(true, Ordering::Relaxed);
        Ok(())
    }
}
impl Default for Frame {
    fn default() -> Self {
        let buf = [0u8; PAGE_SIZE];
        Self {
            page_number: Default::default(),
            data: Arc::new(RwLock::new(buf)),
            pin_count: Default::default(),
            clock_flag: Default::default(),
            dirty: Default::default(),
        }
    }
}

pub struct BufferPool {
    page_file: PathBuf,
    replacement_strategy: ReplacementStrategy,
    frames: Vec<Frame>,
    free_frames: Vec<usize>,
    frame_map: RwLock<HashMap<u64, u64>>,
    storage_mgr: RwLock<StorageManager>,
}

struct FrameTableEntry {
    frame: u64,
    page: u64,
}

impl BufferPool {
    pub fn new(
        num_pages: u64,
        replacement_strategy: ReplacementStrategy,
        page_file_path: PathBuf,
    ) -> Self {
        let mut frames = vec![];
        let mut free_frames = vec![];
        for i in 0..num_pages {
            frames.push(Frame::default());
            free_frames.push(i as usize);
        }
        let storage_mgr =
            StorageManager::open_page_file(&page_file_path).expect("unable to open page file");
        Self {
            page_file: page_file_path,
            replacement_strategy,
            frames,
            free_frames,
            frame_map: RwLock::new(HashMap::new()),
            storage_mgr: RwLock::new(storage_mgr),
        }
    }
    fn find_entry_in_map(&self, page_num: u64) -> Option<FrameTableEntry> {
        match self.frame_map.read() {
            Ok(map) => map.get_key_value(&page_num).map(|entry| FrameTableEntry {
                frame: *entry.0,
                page: *entry.1,
            }),
            Err(_) => None,
        }
    }
    pub fn pin_page(&mut self, page_num: u64) -> DbResult<FrameHandle<'_>> {
        // Check the map first
        if let Some(entry) = self.find_entry_in_map(page_num) {
            let frame = &self.frames[entry.frame as usize];
            frame.pin_count.fetch_add(1, Ordering::Relaxed);
            frame.clock_flag.fetch_or(true, Ordering::Relaxed);
            Ok(FrameHandle { frame })
        } else if !self.free_frames.is_empty() {
            let next_frame_idx = self.free_frames.pop().unwrap();
            let frame = &self.frames[next_frame_idx];
            self.frame_map
                .write()
                .unwrap()
                .insert(next_frame_idx as u64, page_num);

            frame.clock_flag.fetch_or(true, Ordering::Relaxed);

            frame.pin_count.fetch_add(1, Ordering::Relaxed);
            let mut data = frame.data.write().unwrap();
            self.storage_mgr.write().unwrap().read_block(
                page_num,
                data.as_mut_array()
                    .expect("couldn't write to frame buffer!"),
            )?;
            Ok(FrameHandle { frame })
        } else {
            Err(DbError::Unknown)
        }
    }
}
