use std::{
    collections::HashMap,
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    os::unix::fs::FileExt,
    path::{Path, PathBuf},
};
use tracing::{error, info};

use crate::{
    MAGIC_NUMBER, PAGE_SIZE, PageFileFooter, PageId,
    error::{DbError, DbResult},
    page::PageKind,
    page::create_blank_page,
};

#[derive(Debug)]
pub struct FileInfo {
    pub file: File,
    pub metadata: PageFileFooter,
}

#[derive(Debug)]
pub struct StorageState {
    files: HashMap<u32, FileInfo>,
    paths: HashMap<PathBuf, u32>,
    pub next_id: u32,
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct StorageManager {
    pub state: StorageState,
}

#[allow(dead_code)]
impl StorageManager {
    /// Create a StorageManager and open the catalog.db file or create one if it doesn't exist
    pub fn new(base_path: &Path) -> DbResult<Self> {
        let mut file_map = HashMap::new();
        let mut path_map = HashMap::new();
        let catalog_path = base_path.join("catalog.db");

        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&catalog_path)?;

        info!("Opening catalog at: {}", catalog_path.display());

        let metadata = file.metadata().expect("couldn't get file metadata");
        let footer = if metadata.len() >= std::mem::size_of::<PageFileFooter>() as u64 {
            let footer_size = std::mem::size_of::<PageFileFooter>();
            let mut buffer = [0u8; std::mem::size_of::<PageFileFooter>()];
            file.seek(SeekFrom::End(-(footer_size as i64)))?;
            file.read_exact(&mut buffer)?;
            PageFileFooter::from_be_bytes(&buffer)
        } else {
            let page_0 = create_blank_page(0, PageKind::Catalog);
            file.write_all(&page_0)?;
            let footer = PageFileFooter {
                magic_number: MAGIC_NUMBER,
                num_pages: 1,
            };
            file.write_at(&footer.to_be_bytes(), PAGE_SIZE as u64)?;
            footer
        };
        info!("{} page(s) found in catalog", footer.num_pages);

        file_map.insert(
            0,
            FileInfo {
                file,
                metadata: footer,
            },
        );
        path_map.insert(catalog_path, 0);

        Ok(Self {
            state: StorageState {
                files: file_map,
                paths: path_map,
                next_id: 1,
            },
        })
    }

<<<<<<< HEAD
    pub fn file_exists(&self, path: &Path) -> anyhow::Result<bool> {
=======
    pub fn file_exists(&self, path: &Path) -> DbResult<bool> {
>>>>>>> github/main
        let exists = if self.state.paths.contains_key(path) {
            true
        } else {
            path.exists()
        };
        Ok(exists)
    }

    /// Opens a db file and returns a file_id
    pub fn open_or_create_file(&mut self, path: &Path) -> DbResult<u32> {
        if let Some(fid) = self.state.paths.get(path) {
            return Ok(*fid);
        }
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)?;

        self.state
            .paths
            .insert(path.to_path_buf(), self.state.next_id);

        let metadata = file.metadata().expect("couldn't get file metadata");
        let footer = if metadata.len() >= std::mem::size_of::<PageFileFooter>() as u64 {
            let footer_size = std::mem::size_of::<PageFileFooter>();
            let mut buffer = [0u8; std::mem::size_of::<PageFileFooter>()];
            file.seek(SeekFrom::End(-(footer_size as i64)))?;
            file.read_exact(&mut buffer)?;
            PageFileFooter::from_be_bytes(&buffer)
        } else {
            let footer = PageFileFooter {
                magic_number: MAGIC_NUMBER,
                num_pages: 1,
            };
            let new_page = create_blank_page(0, PageKind::Catalog);
            let _ = file.write(&new_page)?;
            file.write_at(&footer.to_be_bytes(), PAGE_SIZE as u64)?;
            footer
        };

        if footer.magic_number != MAGIC_NUMBER {
            error!(
                "Magic number is not correct for page file {}",
                path.display()
            );
            return Err(DbError::CorruptPageFile);
        }

        self.state.files.insert(
            self.state.next_id,
            FileInfo {
                file,
                metadata: footer,
            },
        );
        self.state.next_id += 1;

        Ok(self.state.next_id - 1)
    }

    pub fn read_block(&self, page_id: &PageId, page_handle: &mut [u8; PAGE_SIZE]) -> DbResult<()> {
        tracing::warn!("READ page={} file={}", page_id.page_num, page_id.file_id);
        if let Some(fileinfo) = self.state.files.get(&page_id.file_id) {
            if page_id.page_num >= fileinfo.metadata.num_pages {
                return Err(DbError::PageNotFound);
            }
            let offset = page_id.page_num as usize * PAGE_SIZE;
            fileinfo
                .file
                .read_exact_at(page_handle.as_mut_slice(), offset as u64)?;
            return Ok(());
        }
        Err(DbError::FileNotFound)
    }

    pub fn write_block(&mut self, page_id: &PageId, page_handle: &[u8; PAGE_SIZE]) -> DbResult<()> {
        tracing::warn!("WRITE page={} file={}", page_id.page_num, page_id.file_id);
        // Make sure file exists.
        if !self.state.files.contains_key(&page_id.file_id) {
            return Err(DbError::FileNotFound);
        }

        // Make sure it has that many pages.
        self.ensure_capacity(page_id.file_id, page_id.page_num)?;

        // We now know that we can write here without overwriting footer
        if let Some(fileinfo) = self.state.files.get(&page_id.file_id) {
            let offset = page_id.page_num as usize * PAGE_SIZE;
            fileinfo
                .file
                .write_at(page_handle.as_slice(), offset as u64)?;
            return Ok(());
        }
        tracing::error!("couldnt write block");
        Err(DbError::Unknown)
    }

    pub fn append_empty_block(&mut self, file_id: u32) -> DbResult<u64> {
        if let Some(fileinfo) = self.state.files.get_mut(&file_id) {
            let empty_block = [0u8; PAGE_SIZE];
            let offset = fileinfo.metadata.num_pages as usize * PAGE_SIZE;

            fileinfo.file.write_at(&empty_block, offset as u64)?;

            // Update footer
            fileinfo.metadata.num_pages += 1;
            fileinfo.file.seek(SeekFrom::End(0))?;
            tracing::debug!("updating footer: {:?}", fileinfo);
            _ = fileinfo.file.write(&fileinfo.metadata.to_be_bytes())?;

            return Ok(fileinfo.metadata.num_pages - 1);
        }
        Err(DbError::FileNotFound)
    }

    // TODO: Dont call `append_empty_block` in a loop. Instead, do it all at once... we know how
    // big it needs to be.
    pub fn ensure_capacity(&mut self, file_id: u32, number_of_pages: u64) -> DbResult<()> {
        let mut num_pages = match self.state.files.get(&file_id) {
            Some(info) => info.metadata.num_pages,
            None => return Err(DbError::FileNotFound),
        };
        while num_pages < number_of_pages {
            self.append_empty_block(file_id)?;
            num_pages = self.state.files.get(&file_id).unwrap().metadata.num_pages;
        }
        Ok(())
    }

    pub fn get_next_page_id(&mut self, file_id: u32) -> DbResult<u64> {
        if !self.state.files.contains_key(&file_id) {
            return Err(DbError::FileNotFound);
        }
        self.append_empty_block(file_id)
    }
}
#[cfg(test)]
mod tests {}
