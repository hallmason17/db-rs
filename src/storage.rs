use std::{
    collections::HashMap,
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    os::unix::fs::FileExt,
    path::{Path, PathBuf},
};
use tracing::{error, info};

use crate::{
    MAGIC_NUMBER, PageFileFooter,
    error::{Error, Result},
    ids::{FileId, PageId},
    page::PAGE_SIZE,
    page::PageKind,
    page::create_blank_page,
};

#[derive(Debug)]
pub struct FileInfo {
    pub file: File,
    pub metadata: PageFileFooter,
}

#[derive(Debug)]
pub struct StorageState {}

#[allow(dead_code)]
#[derive(Debug)]
pub struct StorageManager {
    files: HashMap<FileId, FileInfo>,
    paths: HashMap<PathBuf, FileId>,
    pub next_id: FileId,
}

#[allow(dead_code)]
impl StorageManager {
    /// Create a StorageManager and open the catalog.db file or create one if it doesn't exist
    pub fn new(base_path: &Path) -> Result<Self> {
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
        let footer = if metadata.len() >= size_of::<PageFileFooter>() as u64 {
            let footer_size = size_of::<PageFileFooter>();
            let mut buffer = [0u8; size_of::<PageFileFooter>()];
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
            FileId(0),
            FileInfo {
                file,
                metadata: footer,
            },
        );
        path_map.insert("catalog.db".into(), FileId(0));

        Ok(Self {
            files: file_map,
            paths: path_map,
            next_id: FileId(1),
        })
    }
    pub fn get_file_metadata(&self, file_id: FileId) -> Option<&FileInfo> {
        self.files.get(&file_id)
    }

    pub fn file_exists(&self, path: &Path) -> Result<bool> {
        let exists = if self.paths.contains_key(path) {
            true
        } else {
            path.exists()
        };
        Ok(exists)
    }

    pub fn get_num_pages(&self, file_id: FileId) -> Option<u64> {
        Some(self.files.get(&file_id)?.metadata.num_pages)
    }

    /// Opens a db file and returns a file_id
    pub fn open_or_create_file(&mut self, path: &Path) -> Result<FileId> {
        if let Some(fid) = self.paths.get(path) {
            return Ok(*fid);
        }
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)?;

        let metadata = file.metadata().expect("couldn't get file metadata");
        let footer = if metadata.len() >= size_of::<PageFileFooter>() as u64 {
            let footer_size = size_of::<PageFileFooter>();
            let mut buffer = [0u8; size_of::<PageFileFooter>()];
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
            return Err(Error::CorruptPageFile);
        }

        let file_id = self.next_id;
        self.paths.insert(path.to_path_buf(), file_id);
        self.files.insert(
            file_id,
            FileInfo {
                file,
                metadata: footer,
            },
        );
        self.next_id = FileId(self.next_id.0 + 1);

        Ok(file_id)
    }

    pub fn read_block(&self, page_id: &PageId, page_handle: &mut [u8; PAGE_SIZE]) -> Result<()> {
        tracing::warn!("READ page={} file={}", page_id.page_num, page_id.file_id);
        if let Some(fileinfo) = self.files.get(&page_id.file_id) {
            if page_id.page_num >= fileinfo.metadata.num_pages {
                return Err(Error::PageNotFound);
            }
            let offset = page_id.page_num as usize * PAGE_SIZE;
            fileinfo
                .file
                .read_exact_at(page_handle.as_mut_slice(), offset as u64)?;
            return Ok(());
        }
        Err(Error::FileNotFound)
    }

    pub fn write_block(&mut self, page_id: &PageId, page_handle: &[u8; PAGE_SIZE]) -> Result<()> {
        tracing::warn!("WRITE page={} file={}", page_id.page_num, page_id.file_id);
        // Make sure file exists.
        if !self.files.contains_key(&page_id.file_id) {
            return Err(Error::FileNotFound);
        }

        // Make sure it has that many pages.
        self.ensure_capacity(page_id.file_id, page_id.page_num)?;

        // We now know that we can write here without overwriting footer
        if let Some(fileinfo) = self.files.get(&page_id.file_id) {
            let offset = page_id.page_num as usize * PAGE_SIZE;
            fileinfo
                .file
                .write_at(page_handle.as_slice(), offset as u64)?;
            return Ok(());
        }
        error!("couldnt write block");
        Err(Error::Unknown)
    }

    pub fn append_empty_block(&mut self, file_id: FileId) -> Result<u64> {
        if let Some(fileinfo) = self.files.get_mut(&file_id) {
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
        Err(Error::FileNotFound)
    }

    // TODO: Dont call `append_empty_block` in a loop. Instead, do it all at once... we know how
    // big it needs to be.
    pub fn ensure_capacity(&mut self, file_id: FileId, number_of_pages: u64) -> Result<()> {
        let mut num_pages = match self.files.get(&file_id) {
            Some(info) => info.metadata.num_pages,
            None => return Err(Error::FileNotFound),
        };
        while num_pages < number_of_pages {
            self.append_empty_block(file_id)?;
            num_pages = self.files.get(&file_id).unwrap().metadata.num_pages;
        }
        Ok(())
    }

    pub fn get_next_page_id(&mut self, file_id: FileId) -> Result<u64> {
        if !self.files.contains_key(&file_id) {
            return Err(Error::FileNotFound);
        }
        self.append_empty_block(file_id)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn setup() -> (tempfile::TempDir, StorageManager) {
        let dir = tempdir().unwrap();
        let sm = StorageManager::new(dir.path()).unwrap();
        (dir, sm)
    }

    #[test]
    fn open_or_create_returns_file_id() {
        let (dir, mut sm) = setup();
        let path = dir.path().join("test.db");
        let fid = sm.open_or_create_file(&path).unwrap();
        assert_eq!(fid, FileId(1));
        assert!(path.exists());
    }

    #[test]
    fn open_existing_file_returns_same_id() {
        let (dir, mut sm) = setup();
        let path = dir.path().join("test.db");
        let fid1 = sm.open_or_create_file(&path).unwrap();
        let fid2 = sm.open_or_create_file(&path).unwrap();
        assert_eq!(fid1, fid2);
    }

    #[test]
    fn write_then_read_block_roundtrip() {
        let (dir, mut sm) = setup();
        let path = dir.path().join("test.db");
        let fid = sm.open_or_create_file(&path).unwrap();

        let mut data = [0u8; PAGE_SIZE];
        data[0..4].copy_from_slice(&[0xDE, 0xAD, 0xBE, 0xEF]);
        sm.write_block(
            &PageId {
                file_id: fid,
                page_num: 0,
            },
            &data,
        )
        .unwrap();

        let mut buf = [0u8; PAGE_SIZE];
        sm.read_block(
            &PageId {
                file_id: fid,
                page_num: 0,
            },
            &mut buf,
        )
        .unwrap();
        assert_eq!(buf[0..4], [0xDE, 0xAD, 0xBE, 0xEF]);
    }

    #[test]
    fn write_block_auto_extends_file() {
        let (dir, mut sm) = setup();
        let path = dir.path().join("test.db");
        let fid = sm.open_or_create_file(&path).unwrap();

        // file starts at 1 page; write_block calls ensure_capacity(fid, 15)
        let page = [0u8; PAGE_SIZE];
        sm.write_block(
            &PageId {
                file_id: fid,
                page_num: 15,
            },
            &page,
        )
        .unwrap();

        let foot = sm.files.get(&fid).unwrap().metadata.num_pages;
        assert!(foot >= 15, "file should have at least 15 pages, got {foot}");
    }

    #[test]
    fn read_block_file_not_found() {
        let (_dir, sm) = setup();
        let mut buf = [0u8; PAGE_SIZE];
        let result = sm.read_block(
            &PageId {
                file_id: FileId(999),
                page_num: 0,
            },
            &mut buf,
        );
        assert!(result.is_err());
    }

    #[test]
    fn read_block_page_not_found() {
        let (dir, mut sm) = setup();
        let path = dir.path().join("test.db");
        let fid = sm.open_or_create_file(&path).unwrap();
        let mut buf = [0u8; PAGE_SIZE];
        let result = sm.read_block(
            &PageId {
                file_id: fid,
                page_num: 999,
            },
            &mut buf,
        );
        assert!(matches!(result.unwrap_err(), Error::PageNotFound));
    }

    #[test]
    fn corrupt_magic_number_rejected() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("bad.db");

        // Write a page-sized file with wrong magic number in the footer.
        let mut f = std::fs::File::create(&path).unwrap();
        let page = [0u8; PAGE_SIZE];
        f.write_all(&page).unwrap();
        let bad_footer = PageFileFooter {
            magic_number: 0xBAD,
            num_pages: 1,
        };
        f.write_all(&bad_footer.to_be_bytes()).unwrap();
        drop(f);

        let mut sm = StorageManager::new(dir.path()).unwrap();
        let result = sm.open_or_create_file(&path);
        assert!(matches!(result.unwrap_err(), Error::CorruptPageFile));
    }

    #[test]
    fn ensure_capacity_grows_file() {
        let (dir, mut sm) = setup();
        let path = dir.path().join("test.db");
        let fid = sm.open_or_create_file(&path).unwrap();

        sm.ensure_capacity(fid, 10).unwrap();

        let num = sm.files.get(&fid).unwrap().metadata.num_pages;
        assert_eq!(num, 10);
    }
}
