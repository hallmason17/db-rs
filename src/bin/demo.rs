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
    transaction::Transaction,
};
use sqlparser::parser::Parser;
use std::time::Instant;
use tempdir::TempDir;
use tracing_subscriber::{EnvFilter, Layer, fmt, layer::SubscriberExt, util::SubscriberInitExt};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    setup_logger();
    println!(
        "Insert + seq scan demo.
        Run with RUST_LOG=log_level to see logs.
        Ex: RUST_LOG=info cargo r --bin demo"
    );

    let base_path = TempDir::new("db-rs")?;
    println!("Creating db at {:?}", base_path);

    let sm = StorageManager::new(base_path.path()).unwrap();
    let bp = BufferPool::new(1024, ReplacementStrategy::Clock, sm)?;
    let mut db = Database::open(base_path.path().into(), bp)?;

    let create_sql =
        "CREATE TABLE users (id INT PRIMARY KEY, name VARCHAR(50), email VARCHAR(50) NOT NULL);";

    println!("{}", create_sql);

    let mut binder = Binder::new();
    let planner = Planner::new();

    let parsed = Parser::parse_sql(&sqlparser::dialect::GenericDialect {}, create_sql)?;
    let bound = binder.bind(parsed.first().unwrap().clone(), &db)?;
    let plan = planner.plan(bound);
    let mut txn = Transaction::new(&mut db);
    let mut executor = Executor::new(&mut txn);
    let _ = executor.execute(plan);

    let insert_start = Instant::now();
    {
        for i in 0..100 {
            let insert = format!(
                "INSERT INTO users (id, name, email) VALUES ({},'example{}','ex{}@example.com');",
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
    println!("Inserted 100 rows in {:?}", insert_start.elapsed());

    let start = Instant::now();

    let sql = "select id, name from users where (id < 1000) and (name < 'example5');";
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
