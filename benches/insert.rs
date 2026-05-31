use std::path::PathBuf;

use criterion::{BatchSize, BenchmarkId, Criterion, criterion_group, criterion_main};

use db_rs::{
    buffer_pool::{BufferPool, ReplacementStrategy},
    database::Database,
    ids::TableId,
    storage::StorageManager,
    tables::{ColumnDefinition, TableSchema, Tuple},
    value::{DataType, Value},
};

fn setup_table(name: &str) -> (Database, TableId, TableSchema) {
    let dir = PathBuf::from(format!("./bench_data_{name}"));

    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let sm = StorageManager::new(dir.as_path()).unwrap();

    let bm = BufferPool::new(128, ReplacementStrategy::Clock, sm).unwrap();

    let mut db = Database::open(dir.as_path().into(), bm).unwrap();

    let attributes = vec![
        ColumnDefinition::new("id".to_string(), DataType::Int, true, false).unwrap(),
        ColumnDefinition::new("name".to_string(), DataType::VarChar, false, true).unwrap(),
        ColumnDefinition::new("email".to_string(), DataType::VarChar, true, false).unwrap(),
    ];

    let schema = TableSchema::new(&attributes);

    let table = db.create_table(name, &schema).unwrap();
    (db, table, schema)
}

fn make_record(i: i32, schema: &TableSchema) -> Vec<u8> {
    Tuple::new(&[
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
                b.iter_batched(
                    || setup_table("bench"),
                    |(mut db, table, schema)| {
                        for i in 0..inserts {
                            let record = make_record(i, &schema);

                            std::hint::black_box(db.insert_record(table, &record).unwrap());
                        }
                    },
                    BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

criterion_group!(benches, bench_single_thread_insert);

criterion_main!(benches);
