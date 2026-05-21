use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use crate::{
    buffer_pool::BufferPool,
    catalog::CatalogManager,
    storage::StorageManager,
    tables::{ColumnDefinition, DataType, RecordId, Table, TableSchema},
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

pub struct Database {
    buffer_manager: BufferPool,
    catalog_manager: CatalogManager,
    tables: HashMap<u32, Table>,
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
        let catalog = Table::new("catalog", &get_catalog_schema(), &mut buffer_manager, 0)?;
        let mut tables = HashMap::new();
        tables.insert(0, catalog);
        Ok(Self {
            buffer_manager,
            catalog_manager: cm,
            tables,
            base_path,
        })
    }

    pub fn create_table(&mut self, name: &str, schema: &TableSchema) -> anyhow::Result<u32> {
        let path = self.base_path.join(format!("{name}.db"));
        let fid = self
            .buffer_manager
            .storage_manager
            .open_or_create_file(path.as_path())?;
        self.catalog_manager
            .register_table(name, schema, &mut self.buffer_manager, fid)?;

        let table = Table::new(name, schema, &mut self.buffer_manager, fid)?;
        self.tables.insert(fid, table);
        Ok(self.tables.get(&fid).unwrap().id)
    }

    pub fn insert_record(&mut self, table_id: u32, record: &[u8]) -> anyhow::Result<RecordId> {
        let table = self.tables.get_mut(&table_id).unwrap();
        let rid = table.insert_record(record, &mut self.buffer_manager)?;
        Ok(rid)
    }
}
