use std::sync::Arc;

use db_rs::{
    buffer_pool::{BufferPool, ReplacementStrategy},
    catalog::CatalogManager,
    storage::StorageManager,
    tables::{ColumnDefinition, DataType, TableSchema},
};
use parking_lot::RwLock;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    setup_logger();
    let sm = Arc::new(RwLock::new(
        StorageManager::new(std::env::current_dir().unwrap().as_path()).unwrap(),
    ));
    let bm = Arc::new(BufferPool::new(10, ReplacementStrategy::Clock, sm.clone())?);
    let cat = Arc::new(RwLock::new(CatalogManager::new(sm.clone(), bm.clone())?));

    let attributes = vec![
        ColumnDefinition::new(String::from("id"), DataType::Int, true)?,
        ColumnDefinition::new(String::from("name"), DataType::String, false)?,
        ColumnDefinition::new(String::from("email"), DataType::String, true)?,
    ];
    let schema = TableSchema::new(&attributes);
    cat.write().create_table("users", &schema)?;
    cat.write().create_table("users1", &schema)?;
    bm.flush_all()?;
    Ok(())
}

fn setup_logger() {
    let fmt_layer = fmt::layer()
        .with_file(true)
        .with_line_number(true)
        .with_target(false);
    tracing_subscriber::registry().with(fmt_layer).init();
}
