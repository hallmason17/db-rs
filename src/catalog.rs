use std::{collections::HashMap, path::PathBuf, sync::Arc};

use parking_lot::RwLock;

use crate::{storage_mgr::StorageManager, tables::TableSchema, DbResult};

pub struct CatalogManager {
    tables: HashMap<String, TableSchema>,
    open_files: HashMap<String, u32>,
    storage_manager: Arc<RwLock<StorageManager>>,
}

impl CatalogManager {
    pub fn new(storage_manager: Arc<RwLock<StorageManager>>) -> Self {
        let mut tables = HashMap::new();
        tables.insert(String::from("catalog"), 0);

        Self {
            tables: HashMap::new(),
            open_files: tables,
            storage_manager,
        }
    }

    pub fn create_table(&mut self, name: &str, schema: TableSchema) -> DbResult<()> {
        // 1. Write to catalog.db
        // 2. create name.db file, write schema to page 0
        // 3. insert to in-mem map
        self.tables.insert(name.to_string(), schema);
        Ok(())
    }
}
