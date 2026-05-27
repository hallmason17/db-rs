use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use crate::{
    CATALOG_PAGE_ID,
    buffer_pool::BufferPool,
    catalog::{Catalog, CatalogEntry, CatalogManager},
    error::DbResult,
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

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Copy, Clone)]
pub struct FileId(pub u32);
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Copy, Clone)]
pub struct TableId(pub u32);

#[derive(Debug)]
pub struct Database {
    buffer_manager: BufferPool,
    catalog_manager: CatalogManager,
    tables: HashMap<TableId, Table>,
    table_names: HashMap<String, TableId>,
    base_path: PathBuf,
}
impl Database {
    pub fn create(base_path: PathBuf, storage_manager: &mut StorageManager) -> DbResult<()> {
        let catalog_path = base_path.join("catalog.db");
        let catalog_path = Path::new(&catalog_path);
        storage_manager.open_or_create_file(catalog_path)?;
        Ok(())
    }
    pub fn open(base_path: PathBuf, mut buffer_manager: BufferPool) -> DbResult<Self> {
        let mut tables = HashMap::new();
        let mut table_names = HashMap::new();
        let catalog = CatalogManager::load(&mut buffer_manager)?;
        for entry in catalog.entries() {
            let file_id = buffer_manager
                .storage_manager
                .borrow_mut()
                .open_or_create_file(Path::new(&entry.file_name))?;
            let table = Table::open(TableId(file_id), &entry.table_name, &entry.schema);
            tables.insert(TableId(file_id), table);
            table_names.insert(entry.table_name.clone(), TableId(file_id));
        }
        let cat_table = Table::open(TableId(0), "catalog", &get_catalog_schema());
        tables.insert(TableId(0), cat_table);
        table_names.insert("catalog".into(), TableId(0));
        Ok(Self {
            buffer_manager,
            catalog_manager: catalog,
            tables,
            table_names,
            base_path,
        })
    }

    fn update_catalog(&mut self, catalog_entry: CatalogEntry) -> DbResult<()> {
        let entry_bytes = catalog_entry.to_be_bytes()?;
        tracing::debug!("Encoded cat_entry: {:?}", entry_bytes);

        let mut page = self.buffer_manager.get_page(CATALOG_PAGE_ID)?;
        let mut catalog_page = page.as_catalog_mut()?;
        let rid = catalog_page.insert(&entry_bytes)?;
        tracing::debug!("{:?}", rid);
        Ok(())
    }

    pub fn create_table(&mut self, name: &str, schema: &TableSchema) -> DbResult<TableId> {
        let path = self.base_path.join(format!("{name}.db"));
        if let Some(fid) = self.table_names.get(path.to_str().unwrap()) {
            return Ok(*fid);
        }
        if path.exists() {
            let fid = TableId(
                self.buffer_manager
                    .storage_manager
                    .borrow_mut()
                    .open_or_create_file(path.as_path())?,
            );
            self.table_names.insert(String::from(name), fid);
            let table = Table::open(fid, name, schema);
            self.tables.insert(fid, table);
            return Ok(fid);
        }
        let fid = self
            .buffer_manager
            .storage_manager
            .borrow_mut()
            .open_or_create_file(path.as_path())?;

        let catalog_entry = CatalogEntry {
            table_id: TableId(fid),
            table_name: name.to_string(),
            file_name: format!("{name}.db"),
            schema: schema.clone(),
        };

        self.update_catalog(catalog_entry)?;

        self.table_names.insert(String::from(name), TableId(fid));

        self.catalog_manager.create_table(name, schema)?;

        let table = Table::new(name, schema, &mut self.buffer_manager, fid)?;
        self.tables.insert(TableId(fid), table);
        Ok(self.tables.get(&TableId(fid)).unwrap().id)
    }

    pub fn insert_record(&mut self, table_id: TableId, record: &[u8]) -> DbResult<RecordId> {
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
