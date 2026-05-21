use std::{path::PathBuf, sync::Arc, thread};

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};

use db_rs::{
    buffer_pool::{BufferPool, ReplacementStrategy},
    catalog::CatalogManager,
    storage::StorageManager,
    tables::{ColumnDefinition, DataType, Table, TableSchema, Tuple, Value},
};
use parking_lot::RwLock;

fn setup_table(name: &str) -> Arc<Table> {
    let dir = PathBuf::from(format!("./bench_data_{name}"));

    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let sm = Arc::new(RwLock::new(StorageManager::new(dir.as_path()).unwrap()));

    let bm = Arc::new(BufferPool::new(2 ^ 16, ReplacementStrategy::Clock, sm.clone()).unwrap());

    let cat = Arc::new(RwLock::new(
        CatalogManager::new(sm.clone(), bm.clone()).unwrap(),
    ));

    let attributes = vec![
        ColumnDefinition::new("id".to_string(), DataType::Int, true, false).unwrap(),
        ColumnDefinition::new("name".to_string(), DataType::VarChar, false, true).unwrap(),
        ColumnDefinition::new("email".to_string(), DataType::VarChar, true, false).unwrap(),
    ];

    let schema = TableSchema::new(&attributes);

    Arc::new(Table::new(name, &schema, bm.clone(), cat.clone()).unwrap())
}

fn make_record(i: i32, schema: &TableSchema) -> Vec<u8> {
    Tuple::new(&vec![
        Value::Int(i),
        Value::VarChar(format!("user{i}")),
        Value::VarChar(format!("user{i}@email.com")),
    ])
    .serialize(schema)
}

fn bench_single_thread_insert(c: &mut Criterion) {
    let mut group = c.benchmark_group("single_thread_insert");

    for inserts in [1_000, 10_000] {
        group.bench_with_input(
            BenchmarkId::from_parameter(inserts),
            &inserts,
            |b, &inserts| {
                let table = setup_table(&format!("single_thread_{inserts}"));

                let attributes = vec![
                    ColumnDefinition::new("id".to_string(), DataType::Int, true, false).unwrap(),
                    ColumnDefinition::new("name".to_string(), DataType::VarChar, false, true)
                        .unwrap(),
                    ColumnDefinition::new("email".to_string(), DataType::VarChar, true, false)
                        .unwrap(),
                ];

                let schema = TableSchema::new(&attributes);

                b.iter(|| {
                    for i in 0..inserts {
                        let record = make_record(i as i32, &schema);

                        std::hint::black_box(table.insert_record(&record).unwrap());
                    }
                });
            },
        );
    }

    group.finish();
}

fn bench_multi_thread_single_table(c: &mut Criterion) {
    let mut group = c.benchmark_group("multi_thread_single_table");

    for threads in [2, 4, 8] {
        group.bench_with_input(
            BenchmarkId::from_parameter(threads),
            &threads,
            |b, &threads| {
                let table = setup_table("contended_table");

                let attributes = vec![
                    ColumnDefinition::new("id".to_string(), DataType::Int, true, false).unwrap(),
                    ColumnDefinition::new("name".to_string(), DataType::VarChar, false, true)
                        .unwrap(),
                    ColumnDefinition::new("email".to_string(), DataType::VarChar, true, false)
                        .unwrap(),
                ];

                let schema = Arc::new(TableSchema::new(&attributes));

                b.iter(|| {
                    let mut handles = vec![];

                    for tid in 0..threads {
                        let table = table.clone();
                        let schema = schema.clone();

                        handles.push(thread::spawn(move || {
                            for i in 0..10_000 {
                                let record = make_record((tid * 10_000 + i) as i32, &schema);

                                std::hint::black_box(table.insert_record(&record).unwrap());
                            }
                        }));
                    }

                    for h in handles {
                        h.join().unwrap();
                    }
                });
            },
        );
    }

    group.finish();
}

fn bench_multi_thread_multi_table(c: &mut Criterion) {
    let mut group = c.benchmark_group("multi_thread_multi_table");

    for threads in [2, 4, 8, 12] {
        group.bench_with_input(
            BenchmarkId::from_parameter(threads),
            &threads,
            |b, &threads| {
                b.iter(|| {
                    let mut handles = vec![];

                    for tid in 0..threads {
                        handles.push(thread::spawn(move || {
                            let table = setup_table(&format!("table_{tid}"));

                            let attributes = vec![
                                ColumnDefinition::new("id".to_string(), DataType::Int, true, false)
                                    .unwrap(),
                                ColumnDefinition::new(
                                    "name".to_string(),
                                    DataType::VarChar,
                                    false,
                                    true,
                                )
                                .unwrap(),
                                ColumnDefinition::new(
                                    "email".to_string(),
                                    DataType::VarChar,
                                    true,
                                    false,
                                )
                                .unwrap(),
                            ];

                            let schema = TableSchema::new(&attributes);

                            for i in 0..(10_000 / threads) {
                                let record = make_record(i as i32, &schema);

                                std::hint::black_box(table.insert_record(&record).unwrap());
                            }
                        }));
                    }

                    for h in handles {
                        h.join().unwrap();
                    }
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    //bench_single_thread_insert,
    bench_multi_thread_single_table,
    //bench_multi_thread_multi_table
);

criterion_main!(benches);
