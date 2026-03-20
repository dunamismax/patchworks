use std::hint::black_box;

use criterion::{criterion_group, criterion_main, Criterion};
use patchworks::db::inspector::{inspect_database, read_table_page, read_table_page_for_table};
use patchworks::db::types::{SortDirection, TableQuery, TableSort};
use rusqlite::{params, Connection};
use tempfile::TempDir;

fn bench_query_hot_paths(criterion: &mut Criterion) {
    let temp_dir = TempDir::new().expect("temp dir");
    let db_path = temp_dir.path().join("query-hot-paths.sqlite");
    create_query_fixture(&db_path, 25_000);

    let summary = inspect_database(&db_path).expect("inspect fixture");
    let table = summary
        .tables
        .iter()
        .find(|table| table.name == "items")
        .expect("items table")
        .clone();

    let default_query = TableQuery {
        page: 40,
        page_size: 100,
        sort: None,
    };
    let sorted_query = TableQuery {
        page: 40,
        page_size: 100,
        sort: Some(TableSort {
            column: "label".to_owned(),
            direction: SortDirection::Desc,
        }),
    };

    let mut group = criterion.benchmark_group("query_hot_paths");
    group.sample_size(10);

    group.bench_function("read_table_page/schema_lookup/default_order", |bench| {
        bench.iter(|| {
            read_table_page(
                black_box(db_path.as_path()),
                black_box("items"),
                black_box(&default_query),
            )
            .expect("read table page")
        });
    });

    group.bench_function(
        "read_table_page_for_table/preloaded_schema/default_order",
        |bench| {
            bench.iter(|| {
                read_table_page_for_table(
                    black_box(db_path.as_path()),
                    black_box(&table),
                    black_box(&default_query),
                )
                .expect("read table page with preloaded schema")
            });
        },
    );

    group.bench_function(
        "read_table_page_for_table/preloaded_schema/sorted_order",
        |bench| {
            bench.iter(|| {
                read_table_page_for_table(
                    black_box(db_path.as_path()),
                    black_box(&table),
                    black_box(&sorted_query),
                )
                .expect("read sorted table page with preloaded schema")
            });
        },
    );

    group.finish();
}

fn create_query_fixture(path: &std::path::Path, row_count: usize) {
    let mut connection = Connection::open(path).expect("open fixture db");
    connection
        .execute_batch(
            "
            CREATE TABLE items (
                id INTEGER PRIMARY KEY,
                label TEXT NOT NULL,
                category TEXT,
                quantity INTEGER NOT NULL
            );
            CREATE INDEX idx_items_label ON items(label DESC);
            ",
        )
        .expect("create query fixture schema");

    let transaction = connection.transaction().expect("transaction");
    {
        let mut statement = transaction
            .prepare("INSERT INTO items (id, label, category, quantity) VALUES (?1, ?2, ?3, ?4)")
            .expect("prepare insert");
        for id in 1..=row_count {
            statement
                .execute(params![
                    id as i64,
                    format!("item-{id:05}"),
                    format!("cat-{:02}", id % 32),
                    (id % 500) as i64
                ])
                .expect("insert row");
        }
    }
    transaction.commit().expect("commit fixture rows");
}

criterion_group!(benches, bench_query_hot_paths);
criterion_main!(benches);
