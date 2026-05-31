use std::collections::HashMap;

use crate::{
    CATALOG_PAGE_ID,
    buffer_pool::BufferPool,
    error::DbResult,
    ids::TableId,
    page::{PageAccessor, PageHeaderReader, SlottedPageMut},
    tables::TableSchema,
};

#[derive(Debug, Clone)]
pub struct CatalogEntry {
    pub table_id: TableId,
    pub table_name: String,
    pub file_name: String,
    pub schema: TableSchema,
}
impl CatalogEntry {
    pub fn to_be_bytes(self) -> DbResult<Vec<u8>> {
        let mut bytes = vec![];

        bytes.extend_from_slice(&self.table_id.0.to_be_bytes());

        let mut len = u8::try_from(self.table_name.len())?;
        bytes.extend_from_slice(&len.to_be_bytes());
        bytes.extend_from_slice(self.table_name.as_bytes());

        len = u8::try_from(self.file_name.len())?;
        bytes.extend_from_slice(&len.to_be_bytes());
        bytes.extend_from_slice(self.file_name.as_bytes());

        let blob = self.schema.to_be_bytes()?;
        let blob_len = u16::try_from(blob.len())?;
        bytes.extend_from_slice(&blob_len.to_be_bytes());
        bytes.extend_from_slice(&blob);

        Ok(bytes)
    }

    pub fn from_be_bytes(bytes: &[u8]) -> DbResult<CatalogEntry> {
        let data = bytes;
        let table_id = TableId(u32::from_be_bytes(data[0..4].try_into().unwrap()));
        let data = &data[4..];
        let mut len = data[0] as usize;
        let data = &data[1..];
        let table_name = String::from_utf8_lossy(&data[0..len]).to_string();
        let data = &data[len..];
        len = data[0] as usize;
        let file_name = String::from_utf8_lossy(&data[1..=len]).to_string();
        let data = &data[len + 1..];
        let len = u16::from_be_bytes([data[0], data[1]]) as usize;
        let data = &data[2..];
        let schema = TableSchema::from_be_bytes(&data[0..len])?;
        let s = Self {
            table_id,
            table_name,
            file_name,
            schema,
        };
        tracing::debug!("{:?}", s);
        Ok(s)
    }
}

pub trait Catalog {
    fn create_table(&mut self, catalog_entry: &CatalogEntry) -> DbResult<()>;
    fn get_schema(&self, name: &str) -> Option<&TableSchema>;
    fn list_tables(&self) -> Vec<String>;
    fn drop_table(&mut self) -> DbResult<()>;
}

#[derive(Debug)]
pub struct CatalogManager {
    tables: HashMap<String, CatalogEntry>,
}
impl CatalogManager {
    pub fn load(buffer_manager: &mut BufferPool) -> DbResult<Self> {
        let mut tables = HashMap::new();
        let mut page = buffer_manager.get_page(CATALOG_PAGE_ID)?;
        let mut catalog = page.as_catalog_mut()?;
        tracing::debug!("{} entries in catalog!", catalog.header().num_entries());
        for idx in 0..catalog.header().num_entries() {
            let entry = catalog.get_slot_mut(idx)?;
            if let Some(bytes) = entry {
                let cat_entry = CatalogEntry::from_be_bytes(bytes)?;
                tracing::debug!(
                    "Deserialized Table: {}, Schema: {:?}",
                    cat_entry.table_name,
                    cat_entry.schema
                );
                tables.insert(cat_entry.table_name.clone(), cat_entry);
            }
        }

        tracing::debug!("Catalog tables: {:?}", tables);
        let cm = Self { tables };

        Ok(cm)
    }

    pub fn get_num_tables(&self) -> usize {
        self.tables.len()
    }

    pub fn entries(&self) -> Vec<&CatalogEntry> {
        self.tables.values().collect::<Vec<&CatalogEntry>>()
    }
}

impl Catalog for CatalogManager {
    fn create_table(&mut self, catalog_entry: &CatalogEntry) -> DbResult<()> {
        self.tables
            .insert(catalog_entry.table_name.clone(), catalog_entry.clone());

        Ok(())
    }

    fn get_schema(&self, name: &str) -> Option<&TableSchema> {
        match self.tables.get(name) {
            Some(entry) => Some(&entry.schema),
            None => None,
        }
    }

    fn list_tables(&self) -> Vec<String> {
        self.tables.keys().map(String::from).collect::<Vec<_>>()
    }

    fn drop_table(&mut self) -> DbResult<()> {
        todo!()
    }
}
