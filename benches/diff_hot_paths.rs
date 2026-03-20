use std::hint::black_box;

use criterion::{criterion_group, criterion_main, Criterion};
use patchworks::db::differ::diff_databases;
use patchworks::db::inspector::inspect_database;
use patchworks::diff::data::diff_table;
use rusqlite::{params, Connection};
use tempfile::TempDir;

fn bench_diff_hot_paths(criterion: &mut Criterion) {
    let temp_dir = TempDir::new().expect("temp dir");
    let left_path = temp_dir.path().join("diff-left.sqlite");
    let right_path = temp_dir.path().join("diff-right.sqlite");
    create_diff_fixture(&left_path, &right_path, 20_000);

    let left_summary = inspect_database(&left_path).expect("inspect left fixture");
    let right_summary = inspect_database(&right_path).expect("inspect right fixture");
    let left_table = left_summary.tables.first().expect("left table").clone();
    let right_table = right_summary.tables.first().expect("right table").clone();

    let mut group = criterion.benchmark_group("diff_hot_paths");
    group.sample_size(10);

    group.bench_function("diff_table/streaming_row_diff", |bench| {
        bench.iter(|| {
            diff_table(
                black_box(left_path.as_path()),
                black_box(right_path.as_path()),
                black_box(&left_table),
                black_box(&right_table),
            )
            .expect("diff table")
        });
    });

    group.bench_function("diff_databases/end_to_end", |bench| {
        bench.iter(|| {
            diff_databases(
                black_box(left_path.as_path()),
                black_box(right_path.as_path()),
            )
            .expect("diff databases")
        });
    });

    group.finish();
}

fn create_diff_fixture(
    left_path: &std::path::Path,
    right_path: &std::path::Path,
    row_count: usize,
) {
    let mut left = Connection::open(left_path).expect("open left db");
    let mut right = Connection::open(right_path).expect("open right db");
    let schema = "
        CREATE TABLE items (
            id INTEGER PRIMARY KEY,
            label TEXT,
            quantity INTEGER NOT NULL,
            payload BLOB
        );
    ";

    left.execute_batch(schema).expect("create left schema");
    right.execute_batch(schema).expect("create right schema");

    let left_transaction = left.transaction().expect("left transaction");
    {
        let mut statement = left_transaction
            .prepare("INSERT INTO items (id, label, quantity, payload) VALUES (?1, ?2, ?3, ?4)")
            .expect("prepare left insert");
        for id in 1..=row_count {
            statement
                .execute(params![
                    id as i64,
                    format!("item-{id:05}"),
                    (id % 1_000) as i64,
                    vec![(id % 251) as u8, ((id + 17) % 251) as u8]
                ])
                .expect("insert left row");
        }
    }
    left_transaction.commit().expect("commit left fixture");

    let right_transaction = right.transaction().expect("right transaction");
    {
        let mut statement = right_transaction
            .prepare("INSERT INTO items (id, label, quantity, payload) VALUES (?1, ?2, ?3, ?4)")
            .expect("prepare right insert");
        for id in 1..=row_count {
            if id % 17 == 0 {
                continue;
            }

            let label = if id % 11 == 0 {
                format!("item-{id:05}-updated")
            } else {
                format!("item-{id:05}")
            };
            let quantity = if id % 13 == 0 {
                (id % 1_000) as i64 + 5
            } else {
                (id % 1_000) as i64
            };
            let payload = if id % 19 == 0 {
                vec![0xFF, (id % 251) as u8]
            } else {
                vec![(id % 251) as u8, ((id + 17) % 251) as u8]
            };

            statement
                .execute(params![id as i64, label, quantity, payload])
                .expect("insert right row");
        }

        for offset in 1..=2_000 {
            let id = row_count + offset;
            statement
                .execute(params![
                    id as i64,
                    format!("new-item-{id:05}"),
                    (id % 1_000) as i64,
                    vec![0xAA, (id % 251) as u8]
                ])
                .expect("insert added right row");
        }
    }
    right_transaction.commit().expect("commit right fixture");
}

criterion_group!(benches, bench_diff_hot_paths);
criterion_main!(benches);
