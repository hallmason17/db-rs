use db_rs::{
    buffer_pool::{BufferPool, ReplacementStrategy},
    database::Database,
    storage::StorageManager,
    tables::{ColumnDefinition, TableSchema, Tuple},
    value::{DataType, Value},
};
use tracing_subscriber::{EnvFilter, Layer, fmt, layer::SubscriberExt, util::SubscriberInitExt};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    setup_logger();
    let sm = StorageManager::new(std::env::current_dir().unwrap().as_path()).unwrap();
    let bp = BufferPool::new(1024, ReplacementStrategy::Clock, sm)?;
    let mut db = Database::open(std::env::current_dir().unwrap().as_path().into(), bp)?;

    let attributes = vec![
        ColumnDefinition::new(String::from("id"), DataType::Int, true, false)?,
        ColumnDefinition::new(String::from("name"), DataType::VarChar, false, true)?,
        ColumnDefinition::new(String::from("email"), DataType::VarChar, true, false)?,
    ];
    let schema = TableSchema::new(&attributes);

    let tuple = Tuple::new(&[
        Value::Int(1),
        Value::VarChar("mason".to_string()),
        Value::VarChar("hallmason17".to_string()),
    ]);

    let table = db.create_table("users", &schema)?;

    let record = tuple.serialize(&schema);

    for _ in 0..100000 {
        db.insert_record(table, &record)?;
    }

    Ok(())
}

fn setup_logger() {
    let fmt_layer = fmt::layer()
        .with_file(true)
        .with_line_number(true)
        .with_target(false)
        .with_filter(EnvFilter::from_default_env());
    tracing_subscriber::registry().with(fmt_layer).init();
}
