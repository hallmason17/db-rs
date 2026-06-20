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
use db_rs::{
    buffer_pool::{BufferPool, ReplacementStrategy},
    database::Database,
    execution::executor::Executor,
    planner::{binder::Binder, plan::Planner},
    storage::StorageManager,
    tables::{ColumnDefinition, TableSchema, Tuple},
    transaction::Transaction,
    value::{DataType, Value},
};
use sqlparser::parser::Parser;
use std::time::Instant;
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

    let mut binder = Binder::new();
    let planner = Planner::new();

    let insert_start = Instant::now();
    {
        let mut txn = Transaction::new(&mut db);
        for i in 0..100000 {
            let tuple = Tuple::new(vec![
                Value::Int(i),
                Value::VarChar(format!("mason{i}").into()),
                Value::VarChar(format!("masonh{i}@example.com").into()),
            ]);
            let record = tuple.serialize(&schema);
            txn.insert(table, &record)?;
        }
    }
    println!("Inserted 100000 rows in {:?}", insert_start.elapsed());

    let start = Instant::now();

    let sql = "select id, name from users;";
    let parsed = Parser::parse_sql(&sqlparser::dialect::GenericDialect {}, sql)?;
    let bound = binder.bind(parsed.first().unwrap().clone(), &db)?;
    let plan = planner.plan(bound);
    let mut txn = Transaction::new(&mut db);
    let mut executor = Executor::new(&mut txn);

    let rows = executor.execute(plan)?;
    println!("Returned {:?} rows in {:?}", rows.len(), start.elapsed());

    for row in rows {
        println!("{row:?}");
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
