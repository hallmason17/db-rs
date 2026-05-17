use std::{collections::HashMap, path::PathBuf, sync::Arc};

use parking_lot::RwLock;

use crate::{
    CATALOG_PAGE_ID,
    buffer_pool::BufferPool,
    page::{PageAccessor, PageHeaderReader, SlottedPageMut},
    storage::StorageManager,
    tables::TableSchema,
};

#[derive(Debug)]
pub struct CatalogEntry {
    table_name: String,
    file_name: String,
    schema: Vec<u8>,
}
impl CatalogEntry {
    pub fn to_be_bytes(self) -> anyhow::Result<Vec<u8>> {
        let mut bytes = vec![];

        let mut len = u8::try_from(self.table_name.len())?;
        bytes.extend_from_slice(&len.to_be_bytes());
        bytes.extend_from_slice(self.table_name.as_bytes());

        len = u8::try_from(self.file_name.len())?;
        bytes.extend_from_slice(&len.to_be_bytes());
        bytes.extend_from_slice(self.file_name.as_bytes());

        let blob_len = u16::try_from(self.schema.len())?;
        bytes.extend_from_slice(&blob_len.to_be_bytes());
        bytes.extend_from_slice(&self.schema);

        Ok(bytes)
    }

    pub fn from_be_bytes(bytes: &[u8]) -> CatalogEntry {
        let data = bytes;
        let mut len = data[0] as usize;
        let table_name = String::from_utf8_lossy(&data[1..=len]).to_string();
        tracing::debug!("Table name: {}, len: {}", table_name, len);
        let data = &data[len + 1..];
        len = data[0] as usize;
        let file_name = String::from_utf8_lossy(&data[1..=len]).to_string();
        let data = &data[len + 1..];
        let len = u16::from_be_bytes([data[0], data[1]]) as usize;
        let data = &data[2..];
        Self {
            table_name,
            file_name,
            schema: data[0..len].to_vec(),
        }
    }
}

/*
fn get_catalog_schema() -> [ColumnDefinition; 3] {
    [
        ColumnDefinition {
            data_type: DataType::String,
            is_key: true,
            name: String::from("table_name"),
        },
        ColumnDefinition {
            data_type: DataType::String,
            is_key: false,
            name: String::from("file_name"),
        },
        ColumnDefinition {
            data_type: DataType::Blob,
            is_key: false,
            name: String::from("schema_blob"),
        },
    ]
}
*/

#[derive(Debug)]
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
        let mut tables = HashMap::new();
        let mut open_files = HashMap::new();
        open_files.insert(String::from("catalog"), 0);
        let bm = buffer_manager.clone();
        let mut page = bm.get_page(CATALOG_PAGE_ID)?;
        let mut catalog = page.as_catalog_mut()?;
        tracing::debug!("{} entries in catalog!", catalog.header().num_entries());
        for idx in 0..catalog.header().num_entries() {
            let entry = catalog.get_slot_mut(idx)?;
            if let Some(bytes) = entry {
                let cat_entry = CatalogEntry::from_be_bytes(bytes);
                tracing::debug!("{:?}", cat_entry);
                let schema = TableSchema::from_be_bytes(&cat_entry.schema)?;
                tracing::debug!(
                    "Deserialized Table: {}, Schema: {:?}",
                    cat_entry.table_name,
                    schema
                );
                tables.insert(cat_entry.table_name.clone(), schema);
            }
        }

        tracing::debug!("Catalog tables: {:?}", tables);
        let cm = Self {
            tables,
            open_files,
            storage_manager,
            buffer_manager,
        };

        Ok(cm)
    }
    pub fn create_table(&mut self, name: &str, schema: &TableSchema) -> anyhow::Result<u32> {
        let path = self.create_table_file_name(name);
        if let Some(fid) = self.open_files.get(path.to_str().unwrap()) {
            return Ok(*fid);
        }

        if !self.storage_manager.read().file_exists(&path)? {
            let fid = self.storage_manager.write().open_or_create_file(&path)?;
            self.open_files.insert(name.to_string(), fid);

            self.tables.insert(name.to_string(), schema.clone());
            let mut page = self.buffer_manager.get_page(CATALOG_PAGE_ID)?;
            let mut catalog = page.as_catalog_mut()?;
            let catalog_entry = CatalogEntry {
                table_name: name.to_string(),
                file_name: path.display().to_string(),
                schema: schema.to_be_bytes()?,
            };
            let entry_bytes = catalog_entry.to_be_bytes()?;
            tracing::debug!("Encoded cat_entry: {:?}", entry_bytes);
            // 1. Write to catalog.db
            catalog.insert(&entry_bytes)?;

            // 3. insert to in-mem map
            Ok(fid)
        } else {
            let fid = self.storage_manager.write().open_or_create_file(&path)?;
            self.open_files.insert(name.to_string(), fid);

            self.tables.insert(name.to_string(), schema.clone());
            Ok(fid)
        }
    }
    #[must_use]
    pub fn get_num_tables(&self) -> usize {
        self.tables.len()
    }

    fn create_table_file_name(&self, table_name: &str) -> PathBuf {
        self.storage_manager
            .read()
            .base_path
            .join(format!("{table_name}.db"))
    }

    pub fn get_file_id(&self, table_name: &str) -> anyhow::Result<u32> {
        match self.open_files.get(table_name) {
            None => self
                .storage_manager
                .write()
                .open_or_create_file(self.create_table_file_name(table_name).as_path()),
            Some(fid) => Ok(*fid),
        }
    }
}
