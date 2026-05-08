use std::{collections::HashMap, hash::Hash, sync::Arc};

use parking_lot::RwLock;

use crate::{
    CATALOG_PAGE_ID, PageId, buffer_pool::BufferPool, page::PageHeaderReader,
    storage::StorageManager, tables::TableSchema,
};

pub struct CatalogManager {
    tables: HashMap<String, TableSchema>,
    open_files: HashMap<String, u32>,
    storage_manager: Arc<RwLock<StorageManager>>,
    buffer_manager: Arc<BufferPool>,
}

impl CatalogManager {
    pub fn new(
        storage_manager: Arc<RwLock<StorageManager>>,
        buffer_manager: Arc<BufferPool>,
    ) -> anyhow::Result<Self> {
        let mut open_files = HashMap::new();
        open_files.insert(String::from("catalog"), 0);
        let bm = buffer_manager.clone();
        let mut page = bm.get_page(&CATALOG_PAGE_ID)?;
        let mut catalog = page.as_catalog_mut();
        for idx in 0..catalog.header().num_entries() {
            let _ = catalog.get_slot_mut(idx)?.unwrap();
        }

        let cm = Self {
            tables: HashMap::new(),
            open_files,
            storage_manager,
            buffer_manager,
        };

        Ok(cm)
    }
    pub fn create_table(&mut self, name: &str, schema: &TableSchema) -> anyhow::Result<()> {
        let file_name = format!("{}.db", name);
        let path = self.storage_manager.read().base_path.join(file_name);
        if path.exists() {
            return Ok(());
        }

        let mut page = self.buffer_manager.get_page(&CATALOG_PAGE_ID)?;
        let mut catalog = page.as_catalog_mut();
        self.tables.insert(name.to_string(), schema.clone());
        let schema_bytes = schema.as_bytes();
        // 1. Write to catalog.db
        catalog.insert(&schema_bytes)?;
        // 2. create name.db file, write schema to page 0
        let fid = self.storage_manager.write().open_or_create_file(&path)?;
        self.open_files
            .insert(String::from(path.to_str().unwrap()), fid);

        let mut page = self.buffer_manager.get_page(&PageId {
            file_id: fid,
            page_num: 0,
        })?;
        let mut page = page.as_heap_mut();
        page.insert(&schema_bytes)?;

        // 3. insert to in-mem map
        self.tables.insert(name.to_string(), schema.clone());
        Ok(())
    }
    pub fn get_num_tables(&self) -> usize {
        self.tables.len()
    }
}
