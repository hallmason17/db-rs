use std::{
    collections::HashMap,
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    os::unix::fs::FileExt,
    path::{Path, PathBuf},
};
use tracing::{debug, error, info};

use crate::{
    page::create_blank_page, page::PageKind, DbError, DbResult, PageFileFooter, PageId,
    MAGIC_NUMBER, PAGE_SIZE,
};

#[derive(Debug)]
struct FileInfo {
    file: File,
    metadata: PageFileFooter,
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct StorageManager {
    file_map: HashMap<u32, FileInfo>,
    path_map: HashMap<PathBuf, u32>,
    next_id: u32,
    pub base_path: PathBuf,
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
            file_map,
            path_map,
            next_id: 1,
            base_path: base_path.to_path_buf(),
        })
    }

    pub fn file_exists(&self, path: &Path) -> anyhow::Result<bool> {
        let full_path = self.base_path.join(path);
        let exists = if self.path_map.get(&full_path).is_some() {
            true
        } else {
            path.exists()
        };
        Ok(exists)
    }

    /// Opens a db file and returns a file_id
    pub fn open_or_create_file(&mut self, path: &Path) -> anyhow::Result<u32> {
        let full_path = self.base_path.join(path);
        //println!("Opening file at: {}", full_path.display());
        if let Some(id) = self.path_map.get(&full_path) {
            return Ok(*id);
        }
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&full_path)?;
        let id = self.next_id;

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
                full_path.display()
            );
            return Err(DbError::CorruptPageFile.into());
        }

        self.file_map.insert(
            id,
            FileInfo {
                file,
                metadata: footer,
            },
        );
        self.path_map.insert(full_path, id);
        self.next_id += 1;
        debug!("PathMap: {:?}", self.path_map);
        Ok(id)
    }

    pub fn read_block(&self, page_id: &PageId, page_handle: &mut [u8; PAGE_SIZE]) -> DbResult<()> {
        if let Some(fileinfo) = self.file_map.get(&page_id.file_id) {
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

    pub fn write_block(
        &mut self,
        page_id: &PageId,
        page_handle: &[u8; PAGE_SIZE],
    ) -> anyhow::Result<()> {
        // Make sure file exists.
        if !self.file_map.contains_key(&page_id.file_id) {
            return Err(DbError::FileNotFound.into());
        }

        // Make sure it has that many pages.
        self.ensure_capacity(page_id.file_id, page_id.page_num)?;

        // We now know that we can write here without overwriting footer
        if let Some(fileinfo) = self.file_map.get(&page_id.file_id) {
            let offset = page_id.page_num as usize * PAGE_SIZE;
            fileinfo
                .file
                .write_at(page_handle.as_slice(), offset as u64)?;
            return Ok(());
        }
        Err(DbError::Unknown.into())
    }

    pub fn append_empty_block(&mut self, file_id: u32) -> anyhow::Result<u64> {
        if let Some(fileinfo) = self.file_map.get_mut(&file_id) {
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
        Err(DbError::FileNotFound.into())
    }

    // TODO: Dont call `append_empty_block` in a loop. Instead, do it all at once... we know how
    // big it needs to be.
    pub fn ensure_capacity(&mut self, file_id: u32, number_of_pages: u64) -> anyhow::Result<()> {
        let mut num_pages = match self.file_map.get(&file_id) {
            Some(info) => info.metadata.num_pages,
            None => return Err(DbError::FileNotFound.into()),
        };
        while num_pages < number_of_pages {
            self.append_empty_block(file_id)?;
            num_pages = self.file_map.get(&file_id).unwrap().metadata.num_pages;
        }
        Ok(())
    }

    pub fn get_next_page_id(&mut self, file_id: u32) -> anyhow::Result<u64> {
        if !self.file_map.contains_key(&file_id) {
            return Err(DbError::FileNotFound.into());
        }
        self.append_empty_block(file_id)
    }
    /*
     *
    pub fn create_page_file(path: &Path) -> DbResult<()> {
        if !path.exists() {
            let mut file = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(path)?;
            let footer_size = std::mem::size_of::<PageFileFooter>();
            let mut page = vec![0u8; PAGE_SIZE + footer_size];
            let footer = PageFileFooter { num_pages: 1 };
            page[PAGE_SIZE..].copy_from_slice(footer.as_bytes());
            file.write_all(&page)?;
            file.flush()?;
        }
        Ok(())
    }
    pub fn open_page_file(path: &Path) -> DbResult<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .map_err(|_| {
                let table_name = path.file_stem().unwrap_or_default().to_string_lossy();
                DbError::TableNotFound(table_name.to_string())
            })?;

        let mut file = file;
        let footer_size = std::mem::size_of::<PageFileFooter>();
        let mut buffer = [0u8; size_of::<PageFileFooter>()];

        _ = file.seek(SeekFrom::End(-(footer_size as i64)));
        file.read_exact(&mut buffer)?;

        let footer =
            PageFileFooter::read_from_bytes(&buffer).map_err(|_| DbError::CorruptPageFile)?;

        Ok(Self {
            file_map: HashMap::new(),
            next_id: 0,
            page_file_path: path.to_path_buf(),
            footer,
        })
    }
    pub fn destroy_page_file(&self, _file_id: u32) -> DbResult<()> {
        if self.page_file_path.exists() {
            std::fs::remove_file(&self.page_file_path)?;
        }
        Ok(())
    }
     */
}
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_create_page_file() {
        let base_path = tempdir().unwrap();
        let file_name = Path::new("test.page");
        let full_path = base_path.path().join(file_name);

        let mut sm = StorageManager::new(base_path.path()).expect("couldnt init storage manager");
        sm.open_or_create_file(file_name)
            .expect("Failed to create page file");

        assert!(full_path.exists());
        let meta = std::fs::metadata(&full_path).unwrap();
        let expected_size = PAGE_SIZE + std::mem::size_of::<PageFileFooter>();

        assert_eq!(meta.len() as usize, expected_size);
    }

    /*
    #[test]
    fn read_block_works() {
        let base_path = tempdir().unwrap();
        let mut sm = StorageManager::new(base_path.path()).expect("couldnt init storage manager");
        let path = Path::new("test.page");
        let fileid = sm.open_or_create_file(&path).unwrap();
        let mut page_handle = Box::new([1u8; PAGE_SIZE]);
        sm.read_block(
            &PageId {
                file_id: fileid,
                page_num: 0,
            },
            &mut page_handle,
        )
        .expect("Failed to read block");
        assert_eq!(&page_handle.as_slice(), &[0u8; 4096]);
    }

    #[test]
    fn write_block_works() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test3.page");

        StorageManager::create_page_file(&path).unwrap();

        let mut pagefile = StorageManager::open_page_file(&path).expect("Failed to open page file");
        let mut page_handle = Box::new([1u8; PAGE_SIZE]);
        pagefile
            .write_block(
                &PageId {
                    file_id: 0,
                    page_num: 0,
                },
                &page_handle,
            )
            .expect("Couldn't write to block 0");

        pagefile
            .read_block(
                &PageId {
                    file_id: 0,
                    page_num: 0,
                },
                &mut page_handle,
            )
            .expect("Failed to read block");
        assert_eq!(&page_handle.as_slice(), &[1u8; 4096]);
    }

    #[test]
    fn append_empty_works() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test3.page");

        StorageManager::create_page_file(&path).unwrap();

        let mut pagefile = StorageManager::open_page_file(&path).expect("Failed to open page file");
        pagefile
            .append_empty_block(0)
            .expect("Couldn't append empty");

        assert_eq!(
            pagefile.page_file.metadata().unwrap().len(),
            2 * PAGE_SIZE as u64 + size_of::<PageFileFooter>() as u64
        );
        let mut page_handle = Box::new([1u8; PAGE_SIZE]);
        pagefile
            .read_block(
                &PageId {
                    file_id: 0,
                    page_num: 1,
                },
                &mut page_handle,
            )
            .expect("Failed to read block");
        assert_eq!(&page_handle.as_slice(), &[0u8; 4096]);
    }

    #[test]
    fn ensure_capacity_works() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test4.page");

        StorageManager::create_page_file(&path).unwrap();

        let mut pagefile = StorageManager::open_page_file(&path).expect("Failed to open page file");
        pagefile
            .ensure_capacity(0, 15)
            .expect("Couldn't ensure capacity");
        assert_eq!(pagefile.footer.num_pages, 15);
        assert_eq!(
            pagefile.page_file.metadata().unwrap().len(),
            PAGE_SIZE as u64 * 15 + size_of::<PageFileFooter>() as u64
        );
    }
     */
}
