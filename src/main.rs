use std::time::Instant;

use db_rs::{
    buffer_pool::{BufferPool, ReplacementStrategy},
    database::Database,
    executor::Executor,
    expr::Expr::{self, AttrRef},
    plan::{PlanNode, QueryPlan},
    storage::StorageManager,
    tables::{ColumnDefinition, TableSchema},
    transaction::Transaction,
    value::{DataType, Value},
};
use tracing_subscriber::{EnvFilter, Layer, fmt, layer::SubscriberExt, util::SubscriberInitExt};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    setup_logger();
    println!(
        "Insert + seq scan demo.
        Run with RUST_LOG=log_level to see logs."
    );

    let base_path = std::env::current_dir().unwrap();

    let sm = StorageManager::new(base_path.as_path()).unwrap();
    let bp = BufferPool::new(1024, ReplacementStrategy::Clock, sm)?;
    let mut db = Database::open(base_path.clone(), bp)?;

    let attributes = vec![
        ColumnDefinition::new(String::from("id"), DataType::Int, true, false)?,
        ColumnDefinition::new(String::from("name"), DataType::VarChar, false, true)?,
        ColumnDefinition::new(String::from("email"), DataType::VarChar, true, false)?,
    ];
    let schema = TableSchema::new(&attributes);

    let table = db.create_table("users", &schema)?;

    /*
    let mut tuples = vec![];
    for i in 0..10000 {
        let tuple = Tuple::new(&[
            Value::Int(1),
            Value::VarChar(format!("mason{i}")),
            Value::VarChar(format!("masonh{i}@example.com")),
        ]);
        tuples.push(tuple)
    }

    for tuple in &tuples {
        db.insert_record(table, &tuple.serialize(&schema))?;
    }
    */

    let start = Instant::now();
    let rows = {
        let table = db.tables.get(&table).unwrap().clone();
        let txn = Transaction::new(&mut db);
        let executor = Executor::new(&txn);
        executor.execute(QueryPlan::Select(PlanNode::SeqScan {
            table,
            filter: Some(Expr::Equal(
                Box::new(AttrRef(0)),
                Box::new(Expr::Constant(Value::Int(1))),
            )),
        }))?
    };

    println!("Found {} row(s):", rows.len());
    for _row in rows {
        //println!("{row:?}");
    }
    println!("Elapsed: {:?}", start.elapsed());

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
