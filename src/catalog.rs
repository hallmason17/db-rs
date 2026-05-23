use std::collections::HashMap;

use crate::{
    CATALOG_PAGE_ID,
    buffer_pool::BufferPool,
    page::{PageAccessor, PageHeaderReader, SlottedPageMut},
    tables::TableSchema,
};

#[derive(Debug)]
pub struct CatalogEntry {
    pub table_name: String,
    pub file_name: String,
    pub schema: Vec<u8>,
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

pub trait Catalog {
    fn create_table(&mut self, name: &str, schema: &TableSchema) -> anyhow::Result<()>;
    fn get_schema(&self, name: &str) -> Option<&TableSchema>;
    fn list_tables(&self) -> Vec<String>;
    fn drop_table(&mut self) -> anyhow::Result<()>;
}

#[derive(Debug)]
pub struct CatalogManager {
    tables: HashMap<String, TableSchema>,
}
impl CatalogManager {
    pub fn new(buffer_manager: &mut BufferPool) -> anyhow::Result<Self> {
        let mut tables = HashMap::new();
        let mut page = buffer_manager.get_page(CATALOG_PAGE_ID)?;
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
        let cm = Self { tables };

        Ok(cm)
    }

    pub fn get_num_tables(&self) -> usize {
        self.tables.len()
    }
}

impl Catalog for CatalogManager {
    fn create_table(&mut self, name: &str, schema: &TableSchema) -> anyhow::Result<()> {
        self.tables.insert(name.to_string(), schema.clone());

        // 3. insert to in-mem map
        Ok(())
    }

    fn get_schema(&self, name: &str) -> Option<&TableSchema> {
        self.tables.get(name)
    }

    fn list_tables(&self) -> Vec<String> {
        self.tables.keys().map(String::from).collect::<Vec<_>>()
    }

    fn drop_table(&mut self) -> anyhow::Result<()> {
        todo!()
    }
}
