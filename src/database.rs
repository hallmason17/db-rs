/* Copyright (C) 2026 Mason Hall.
 *
 * This file is part of db-rs.
 *
 * db-rs is free software: you can redistribute it and/or modify it under the
 * terms of the GNU General Public License as published by the Free Software
 * Foundation, either version 3 of the License, or (at your option) any later
 * version.
 *
 * db-rs is distributed in the hope that it will be useful, but WITHOUT ANY
 * WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS
 * FOR A PARTICULAR PURPOSE. See the GNU General Public License for more
 * details.
 *
 * You should have received a copy of the GNU General Public License along with
 * db-rs. If not, see <https://www.gnu.org/licenses/>.
 */
use std::{
    collections::HashMap,
    fmt,
    path::{Path, PathBuf},
};

use crate::{
    CATALOG_PAGE_ID,
    buffer_pool::BufferPool,
    catalog::{CatalogEntry, CatalogManager},
    error::Result,
    ids::{FileId, TableId},
    page::SlottedPageMut,
    storage::StorageManager,
    tables::{ColumnDefinition, Table, TableSchema},
    transaction::Transaction,
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

impl fmt::Display for FileId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug)]
pub struct Database {
    pub buffer_manager: BufferPool,
    pub catalog_manager: CatalogManager,
    pub tables: HashMap<TableId, Table>,
    pub table_names: HashMap<String, TableId>,
    base_path: PathBuf,
}
impl Database {
    pub fn create(base_path: PathBuf, storage_manager: &mut StorageManager) -> Result<()> {
        let catalog_path = base_path.join("catalog.db");
        let catalog_path = Path::new(&catalog_path);
        storage_manager.open_or_create_file(catalog_path)?;
        Ok(())
    }
    pub fn open(base_path: PathBuf, mut buffer_manager: BufferPool) -> Result<Self> {
        let mut tables = HashMap::new();
        let mut table_names = HashMap::new();
        let catalog = CatalogManager::load(&mut buffer_manager)?;
        for entry in catalog.entries() {
            let file_id = buffer_manager
                .storage_manager
                .borrow_mut()
                .open_or_create_file(&base_path.join(Path::new(&entry.file_name)))?;
            let table = Table::open(entry.table_id, file_id, &entry.table_name, &entry.schema);
            tables.insert(entry.table_id, table);
            table_names.insert(entry.table_name.clone(), entry.table_id);
        }
        let cat_table = Table::open(TableId(0), FileId(0), "catalog", &get_catalog_schema());
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

    pub fn start_transaction(&'_ mut self) -> Transaction<'_> {
        Transaction { db: self }
    }

    fn update_catalog(&mut self, catalog_entry: CatalogEntry) -> Result<()> {
        self.catalog_manager.create_table(&catalog_entry)?;
        let entry_bytes = catalog_entry.to_be_bytes()?;
        tracing::debug!("Encoded cat_entry: {:?}", entry_bytes);

        let mut page = self.buffer_manager.get_page(CATALOG_PAGE_ID)?;
        let mut catalog_page = page.as_catalog_mut()?;
        let rid = catalog_page.insert(&entry_bytes)?;
        tracing::debug!("{:?}", rid);
        Ok(())
    }

    pub fn create_table(&mut self, name: &str, schema: &TableSchema) -> Result<TableId> {
        let path = self.base_path.join(format!("{name}.db"));
        if let Some(fid) = self.table_names.get(name) {
            return Ok(*fid);
        }
        if path.exists() {
            let fid = self
                .buffer_manager
                .storage_manager
                .borrow_mut()
                .open_or_create_file(path.as_path())?;
            self.table_names.insert(String::from(name), TableId(fid.0));
            let table = Table::open(TableId(fid.0), fid, name, schema);
            self.tables.insert(TableId(fid.0), table);
            return Ok(TableId(fid.0));
        }
        let fid = self
            .buffer_manager
            .storage_manager
            .borrow_mut()
            .open_or_create_file(path.as_path())?;

        let catalog_entry = CatalogEntry {
            table_id: TableId(fid.0),
            table_name: name.to_string(),
            file_name: format!("{name}.db"),
            schema: schema.clone(),
        };

        self.update_catalog(catalog_entry)?;

        self.table_names.insert(String::from(name), TableId(fid.0));

        let table = Table::new(name, schema, &mut self.buffer_manager, fid)?;
        self.tables.insert(TableId(fid.0), table);
        Ok(self.tables.get(&TableId(fid.0)).unwrap().table_id)
    }

    pub fn get_table(&self, name: &str) -> Option<&Table> {
        if let Some(id) = self.table_names.get(name) {
            return self.tables.get(id);
        }
        None
    }
}

impl Drop for Database {
    fn drop(&mut self) {
        let _ = self.buffer_manager.flush_all();
    }
}
