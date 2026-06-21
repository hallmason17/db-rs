/* Copyright (C) 2026 Mason Hall.
 *
 * GNU General Public License v3.0+ (see COPYING or https://www.gnu.org/licenses/gpl-3.0.txt)
 */
use db_rs::{
    buffer_pool::{BufferPool, ReplacementStrategy},
    database::Database,
    execution::executor::Executor,
    planner::{binder::Binder, plan::Planner},
    storage::StorageManager,
    tables::{ColumnDefinition, TableSchema},
    transaction::Transaction,
    value::DataType,
};
use sqlparser::parser::Parser;
use std::time::Instant;
use tempdir::TempDir;
use tracing_subscriber::{EnvFilter, Layer, fmt, layer::SubscriberExt, util::SubscriberInitExt};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    setup_logger();
    println!(
        "Insert + seq scan demo.
        Run with RUST_LOG=log_level to see logs."
    );

    let base_path = TempDir::new("db-rs")?;
    println!("Creating db at {:?}", base_path);

    let sm = StorageManager::new(base_path.path()).unwrap();
    let bp = BufferPool::new(1024, ReplacementStrategy::Clock, sm)?;
    let mut db = Database::open(base_path.path().into(), bp)?;

    let attributes = vec![
        ColumnDefinition::new(String::from("id"), DataType::Int, true, false)?,
        ColumnDefinition::new(String::from("name"), DataType::VarChar, false, true)?,
        ColumnDefinition::new(String::from("email"), DataType::VarChar, true, false)?,
    ];
    let schema = TableSchema::new(&attributes);

    let _ = db.create_table("users", &schema)?;

    let mut binder = Binder::new();
    let planner = Planner::new();

    let insert_start = Instant::now();
    {
        for i in 0..1000 {
            let insert = format!(
                "INSERT INTO users (id, name, email) VALUES ({},'mason{}','mason{}@example.com');",
                i, i, i
            );
            println!("{}", insert);
            let parsed = Parser::parse_sql(&sqlparser::dialect::GenericDialect {}, &insert)?;
            let bound = binder.bind(parsed.first().unwrap().clone(), &db)?;
            let plan = planner.plan(bound);
            let mut txn = Transaction::new(&mut db);
            let mut executor = Executor::new(&mut txn);
            let _ = executor.execute(plan)?;
        }
    }
    println!("Inserted 1000 rows in {:?}", insert_start.elapsed());

    let start = Instant::now();

    let sql = "select id, name from users where (id < 1000) and (name < 'mason5');";
    println!("{}", sql);
    let parsed = Parser::parse_sql(&sqlparser::dialect::GenericDialect {}, sql)?;
    let bound = binder.bind(parsed.first().unwrap().clone(), &db)?;
    let plan = planner.plan(bound);
    let mut txn = Transaction::new(&mut db);
    let mut executor = Executor::new(&mut txn);

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
