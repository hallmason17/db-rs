use std::time::Instant;

use db_rs::{
    buffer_pool::{BufferPool, ReplacementStrategy},
    database::Database,
    execution::executor::Executor,
    expr::Expr::{self, AttrRef},
    planner::{
        binder::Binder,
        planner::{PlanNode, Planner, QueryPlan},
    },
    storage::StorageManager,
    tables::{ColumnDefinition, TableSchema, Tuple},
    transaction::Transaction,
    value::{DataType, Value},
};
use sqlparser::parser::Parser;
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

    let start = Instant::now();

    /*
        let mut tuples = vec![];
        for i in 0..1000 {
            let tuple = Tuple::new(&[
                Value::Int(i),
                Value::VarChar(format!("mason{i}")),
                Value::VarChar(format!("masonh{i}@example.com")),
            ]);
            tuples.push(tuple)
        }

        for tuple in &tuples {
            db.insert_record(table, &tuple.serialize(&schema))?;
        }
    */

    let sql = "select id, name from users;";
    let parsed = Parser::parse_sql(&sqlparser::dialect::GenericDialect {}, sql)?;
    let mut binder = Binder::new();
    let bound = binder.bind(parsed.first().unwrap().clone(), &db)?;
    let planner = Planner::new();
    let plan = planner.plan(bound);
    let txn = Transaction::new(&mut db);
    let executor = Executor::new(&txn);

    let rows = executor.execute(plan)?;
    println!("Returned {:?} rows in {:?}", rows.len(), start.elapsed());
    /*
        for row in rows {
            println!("{row:?}");
        }
    */

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
