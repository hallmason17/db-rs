use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use crate::{
    CATALOG_PAGE_ID,
    buffer_pool::BufferPool,
    catalog::{Catalog, CatalogEntry, CatalogManager},
    page::SlottedPageMut,
    storage::StorageManager,
    tables::{ColumnDefinition, RecordId, Table, TableSchema},
    value::DataType,
};

fn get_catalog_schema() -> TableSchema {
    TableSchema {
        attributes: vec![
            ColumnDefinition {
                name: String::from("table_name"),
                data_type: DataType::VarChar,
                is_key: true,
                is_nullable: false,
            },
            ColumnDefinition {
                name: String::from("file_name"),
                data_type: DataType::VarChar,
                is_key: true,
                is_nullable: false,
            },
            ColumnDefinition {
                name: String::from("schema"),
                data_type: DataType::Blob,
                is_key: false,
                is_nullable: false,
            },
        ],
    }
}

#[derive(Debug)]
pub struct Database {
    buffer_manager: BufferPool,
    catalog_manager: CatalogManager,
    tables: HashMap<u32, Table>,
    open_files: HashMap<String, u32>,
    base_path: PathBuf,
}
impl Database {
    pub fn create(base_path: PathBuf, storage_manager: &mut StorageManager) -> anyhow::Result<()> {
        let catalog_path = base_path.join("catalog.db");
        let catalog_path = Path::new(&catalog_path);
        storage_manager.open_or_create_file(catalog_path)?;
        Ok(())
    }
    pub fn open(base_path: PathBuf, mut buffer_manager: BufferPool) -> anyhow::Result<Self> {
        let cm = CatalogManager::new(&mut buffer_manager)?;
        let catalog = Table::open(0, "catalog", &get_catalog_schema());
        let mut tables = HashMap::new();
        tables.insert(0, catalog);
        let mut open_files = HashMap::new();
        open_files.insert(String::from("catalog.db"), 0);
        Ok(Self {
            buffer_manager,
            catalog_manager: cm,
            tables,
            open_files,
            base_path,
        })
    }

    fn update_catalog(&mut self, catalog_entry: CatalogEntry) -> anyhow::Result<()> {
        let entry_bytes = catalog_entry.to_be_bytes()?;
        tracing::debug!("Encoded cat_entry: {:?}", entry_bytes);

        let mut page = self.buffer_manager.get_page(CATALOG_PAGE_ID)?;
        let mut catalog_page = page.as_catalog_mut()?;
        let rid = catalog_page.insert(&entry_bytes)?;
        tracing::debug!("{:?}", rid);
        Ok(())
    }

    pub fn create_table(&mut self, name: &str, schema: &TableSchema) -> anyhow::Result<u32> {
        let path = self.base_path.join(format!("{name}.db"));
        if let Some(fid) = self.open_files.get(path.to_str().unwrap()) {
            return Ok(*fid);
        }
        if path.exists() {
            let fid = self
                .buffer_manager
                .storage_manager
                .open_or_create_file(path.as_path())?;
            self.open_files.insert(String::from(name), fid);
            let table = Table::open(fid, name, schema);
            self.tables.insert(fid, table);
            return Ok(fid);
        }
        let fid = self
            .buffer_manager
            .storage_manager
            .open_or_create_file(path.as_path())?;

        let catalog_entry = CatalogEntry {
            table_name: name.to_string(),
            file_name: path.display().to_string(),
            schema: schema.to_be_bytes()?,
        };

        self.update_catalog(catalog_entry)?;

        self.open_files.insert(String::from(name), fid);

        self.catalog_manager.create_table(name, schema)?;

        let table = Table::new(name, schema, &mut self.buffer_manager, fid)?;
        self.tables.insert(fid, table);
        Ok(self.tables.get(&fid).unwrap().id)
    }

    pub fn insert_record(&mut self, table_id: u32, record: &[u8]) -> anyhow::Result<RecordId> {
        let table = self.tables.get_mut(&table_id).unwrap();
        let rid = table.insert(record, &mut self.buffer_manager)?;
        Ok(rid)
    }
}

impl Drop for Database {
    fn drop(&mut self) {
        let _ = self.buffer_manager.flush_all();
    }
}
