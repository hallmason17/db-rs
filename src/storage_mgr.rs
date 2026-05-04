use std::{
    collections::HashMap,
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    os::unix::fs::FileExt,
    path::{Path, PathBuf},
};
use zerocopy::{FromBytes, Immutable, IntoBytes};

use crate::{DbError, DbResult, PageId, PAGE_SIZE};

struct FileInfo {
    file: File,
    metadata: PageFileFooter,
}

#[allow(dead_code)]
pub struct StorageManager {
    file_map: HashMap<u32, FileInfo>,
    path_map: HashMap<PathBuf, u32>,
    next_id: u32,
    pub base_path: PathBuf,
}

#[derive(IntoBytes, FromBytes, Immutable)]
#[repr(C)]
pub struct PageFileFooter {
    num_pages: u32,
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
            .open(&catalog_path)?;

        let metadata = file.metadata().expect("couldn't get file metadata");
        let footer = if metadata.len() >= std::mem::size_of::<PageFileFooter>() as u64 {
            let footer_size = std::mem::size_of::<PageFileFooter>();
            let mut buffer = [0u8; std::mem::size_of::<PageFileFooter>()];
            file.seek(SeekFrom::End(-(footer_size as i64)))?;
            file.read_exact(&mut buffer)?;
            PageFileFooter::read_from_bytes(&buffer).unwrap_or(PageFileFooter { num_pages: 0 })
        } else {
            PageFileFooter { num_pages: 0 }
        };

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

    /// Opens a db file and returns a file_id
    pub fn open_file(&mut self, path: &Path) -> DbResult<u32> {
        let full_path = self.base_path.join(path);
        if let Some(id) = self.path_map.get(&full_path) {
            return Ok(*id);
        }
        let mut file = File::open(&full_path)?;
        let id = self.next_id;

        let footer_size = std::mem::size_of::<PageFileFooter>();
        let mut buffer = [0u8; size_of::<PageFileFooter>()];

        _ = file.seek(SeekFrom::End(-(footer_size as i64)));
        file.read_exact(&mut buffer)?;

        let footer =
            PageFileFooter::read_from_bytes(&buffer).map_err(|_| DbError::CorruptPageFile)?;

        self.file_map.insert(
            id,
            FileInfo {
                file,
                metadata: footer,
            },
        );
        self.path_map.insert(full_path, id);
        self.next_id += 1;
        Ok(id)
    }

    pub fn read_block(&self, page_id: &PageId, page_handle: &mut [u8; PAGE_SIZE]) -> DbResult<()> {
        if let Some(fileinfo) = self.file_map.get(&page_id.file_id) {
            if page_id.page_num > (fileinfo.metadata.num_pages - 1) {
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
        if let Some(fileinfo) = self.file_map.get_mut(&page_id.file_id) {
            let offset = page_id.page_num as usize * PAGE_SIZE;
            fileinfo
                .file
                .write_at(page_handle.as_slice(), offset as u64)?;
            fileinfo.file.flush()?;
            return Ok(());
        }
        Err(DbError::FileNotFound)
    }

    pub fn append_empty_block(&mut self, file_id: u32) -> DbResult<()> {
        if let Some(fileinfo) = self.file_map.get_mut(&file_id) {
            let empty_block = [0u8; PAGE_SIZE];
            let offset = fileinfo.metadata.num_pages as usize * PAGE_SIZE;

            fileinfo.file.write_at(&empty_block, offset as u64)?;

            fileinfo.metadata.num_pages += 1;
            fileinfo.file.seek(SeekFrom::End(0))?;
            _ = fileinfo.file.write(fileinfo.metadata.as_bytes())?;

            fileinfo.file.flush()?;
            return Ok(());
        }
        Err(DbError::FileNotFound)
    }

    pub fn ensure_capacity(&mut self, file_id: u32, number_of_pages: u32) -> DbResult<()> {
        let mut num_pages = match self.file_map.get(&file_id) {
            Some(info) => info.metadata.num_pages,
            None => return Err(DbError::FileNotFound),
        };
        while num_pages < number_of_pages {
            self.append_empty_block(file_id)?;
            num_pages = self.file_map.get(&file_id).unwrap().metadata.num_pages;
        }
        Ok(())
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
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.page");

        StorageManager::create_page_file(&path).expect("Failed to create page file");

        assert!(path.exists());
        let meta = std::fs::metadata(&path).unwrap();
        let expected_size = PAGE_SIZE + std::mem::size_of::<PageFileFooter>();

        assert_eq!(meta.len() as usize, expected_size);
    }

    #[test]
    fn test_open_page_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test1.page");

        StorageManager::create_page_file(&path).unwrap();

        let pagefile = StorageManager::open_page_file(&path).expect("Failed to open page file");

        assert_eq!(pagefile.footer.num_pages, 1);
    }

    #[test]
    fn read_block_works() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test2.page");

        StorageManager::create_page_file(&path).unwrap();

        let pagefile = StorageManager::open_page_file(&path).expect("Failed to open page file");
        let mut page_handle = Box::new([1u8; PAGE_SIZE]);
        pagefile
            .read_block(
                &PageId {
                    file_id: 0,
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
}
