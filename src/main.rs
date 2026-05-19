use core::num;
use std::{sync::Arc, thread};

use db_rs::{
    buffer_pool::{BufferPool, ReplacementStrategy},
    catalog::CatalogManager,
    storage::StorageManager,
    tables::{ColumnDefinition, DataType, Table, TableSchema, Tuple, Value},
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
        ColumnDefinition::new(String::from("id"), DataType::Int, true, false)?,
        ColumnDefinition::new(String::from("name"), DataType::VarChar, false, true)?,
        ColumnDefinition::new(String::from("email"), DataType::VarChar, true, false)?,
    ];
    let schema = TableSchema::new(&attributes);

    let table = Arc::new(Table::new("users", &schema, bm.clone(), cat.clone())?);

    let tuple = Tuple::new(&vec![
        Value::Int(1),
        Value::VarChar("mason".to_string()),
        Value::VarChar("hallmason17".to_string()),
    ]);

    let _tuple1 = Tuple::new(&vec![
        Value::Int(1),
        Value::Null,
        Value::VarChar("hallmason17".to_string()),
    ]);
    let _record = tuple.serialize(&schema);
/* */
    let mut threads = vec![];
    let num_threads = 2;
    for _ in 0..num_threads {
        let value = table.clone();
        let thread = thread::spawn(move || {
            let t = value.clone();
                let attributes = vec![
        ColumnDefinition::new(String::from("id"), DataType::Int, true, false).unwrap(),
        ColumnDefinition::new(String::from("name"), DataType::VarChar, false, true).unwrap(),
        ColumnDefinition::new(String::from("email"), DataType::VarChar, true, false).unwrap(),
    ];
    let schema = TableSchema::new(&attributes);
            let tuple1 = Tuple::new(&vec![
                Value::Int(1),
                Value::Null,
                Value::VarChar("hallmason17".to_string()),
            ]);

            for _ in 0..=100000 / num_threads{
                let rid = t.insert_record(&tuple1.serialize(&schema));
                println!("{:?}", rid);
            }
        });
        threads.push(thread);
    }
    for t in threads {
        t.join().unwrap();
    }

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
