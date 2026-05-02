use std::{
    fs::OpenOptions,
    io::{Read, Seek, SeekFrom, Write},
    os::unix::fs::FileExt,
    path::{Path, PathBuf},
};
use zerocopy::{FromBytes, Immutable, IntoBytes};

use crate::{DbError, DbResult, PAGE_SIZE};

#[allow(dead_code)]
pub struct StorageManager {
    page_file: std::fs::File,
    page_file_path: PathBuf,
    footer: PageFileFooter,
}

#[derive(IntoBytes, FromBytes, Immutable)]
#[repr(C)]
pub struct PageFileFooter {
    num_pages: u32,
}

#[allow(dead_code)]
impl StorageManager {
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
            page_file: file,
            page_file_path: path.to_path_buf(),
            footer,
        })
    }
    pub fn destroy_page_file(&self) -> DbResult<()> {
        if self.page_file_path.exists() {
            std::fs::remove_file(&self.page_file_path)?;
        }
        Ok(())
    }

    pub fn read_block(
        &mut self,
        block_num: u64,
        page_handle: &mut [u8; PAGE_SIZE],
    ) -> DbResult<()> {
        let offset = block_num as usize * PAGE_SIZE;
        self.page_file
            .read_exact_at(page_handle.as_mut_slice(), offset as u64)?;
        Ok(())
    }
    pub fn write_block(&mut self, block_num: u64, page_handle: &[u8; PAGE_SIZE]) -> DbResult<()> {
        let offset = block_num as usize * PAGE_SIZE;
        self.page_file
            .write_at(page_handle.as_slice(), offset as u64)?;
        self.page_file.flush()?;
        Ok(())
    }

    pub fn append_empty_block(&mut self) -> DbResult<()> {
        let empty_block = [0u8; PAGE_SIZE];
        let offset = self.footer.num_pages as usize * PAGE_SIZE;
        self.page_file.write_at(&empty_block, offset as u64)?;
        self.footer.num_pages += 1;
        self.page_file.seek(SeekFrom::End(0))?;
        _ = self.page_file.write(self.footer.as_bytes())?;
        self.page_file.flush()?;
        Ok(())
    }

    pub fn ensure_capacity(&mut self, number_of_pages: u32) -> DbResult<()> {
        while self.footer.num_pages < number_of_pages {
            self.append_empty_block()?
        }
        Ok(())
    }
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

        let mut pagefile = StorageManager::open_page_file(&path).expect("Failed to open page file");
        let mut page_handle = PageHandle {
            page_number: 0,
            data: Box::new([1u8; PAGE_SIZE]),
        };
        pagefile
            .read_block(0, &mut page_handle)
            .expect("Failed to read block");
        assert_eq!(&page_handle.data.as_slice(), &[0u8; 4096]);
    }

    #[test]
    fn write_block_works() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test3.page");

        StorageManager::create_page_file(&path).unwrap();

        let mut pagefile = StorageManager::open_page_file(&path).expect("Failed to open page file");
        let mut page_handle = PageHandle {
            page_number: 0,
            data: Box::new([1u8; PAGE_SIZE]),
        };
        pagefile
            .write_block(0, &page_handle)
            .expect("Couldn't write to block 0");

        pagefile
            .read_block(0, &mut page_handle)
            .expect("Failed to read block");
        assert_eq!(&page_handle.data.as_slice(), &[1u8; 4096]);
    }

    #[test]
    fn append_empty_works() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test3.page");

        StorageManager::create_page_file(&path).unwrap();

        let mut pagefile = StorageManager::open_page_file(&path).expect("Failed to open page file");
        pagefile
            .append_empty_block()
            .expect("Couldn't append empty");

        assert_eq!(
            pagefile.page_file.metadata().unwrap().len(),
            2 * PAGE_SIZE as u64 + size_of::<PageFileFooter>() as u64
        );
        let mut page_handle = PageHandle {
            page_number: 0,
            data: Box::new([1u8; PAGE_SIZE]),
        };
        pagefile
            .read_block(1, &mut page_handle)
            .expect("Failed to read block");
        assert_eq!(&page_handle.data.as_slice(), &[0u8; 4096]);
    }

    #[test]
    fn ensure_capacity_works() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test4.page");

        StorageManager::create_page_file(&path).unwrap();

        let mut pagefile = StorageManager::open_page_file(&path).expect("Failed to open page file");
        pagefile
            .ensure_capacity(15)
            .expect("Couldn't ensure capacity");
        assert_eq!(pagefile.footer.num_pages, 15);
        assert_eq!(
            pagefile.page_file.metadata().unwrap().len(),
            PAGE_SIZE as u64 * 15 + size_of::<PageFileFooter>() as u64
        );
    }
}
