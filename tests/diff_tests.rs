use std::path::Path;

use patchworks::db::differ::diff_databases;
use patchworks::db::inspector::{inspect_database, read_table_page};
use patchworks::db::snapshot::SnapshotStore;
use patchworks::db::types::{SortDirection, TableQuery, TableSort};
use rusqlite::Connection;
use tempfile::TempDir;

mod support;

use support::{create_db, create_db_with_sql};

#[test]
fn inspector_reads_schema_and_paged_rows() {
    let temp_dir = TempDir::new().expect("temp dir");
    let db_path = create_db(&temp_dir, "schema-left.sqlite", "schema_left");

    let summary = inspect_database(&db_path).expect("inspect database");
    assert_eq!(summary.tables.len(), 2);
    assert_eq!(summary.views.len(), 0);
    let users = summary
        .tables
        .iter()
        .find(|table| table.name == "users")
        .expect("users table");
    assert_eq!(users.primary_key, vec!["id".to_owned()]);
    assert_eq!(users.row_count, 2);

    let page = read_table_page(
        &db_path,
        "users",
        &TableQuery {
            page: 0,
            page_size: 1,
            sort: Some(TableSort {
                column: "name".to_owned(),
                direction: SortDirection::Desc,
            }),
        },
    )
    .expect("read page");
    assert_eq!(page.rows.len(), 1);
    assert_eq!(page.rows[0][1].display(), "Linus");
}

#[test]
fn schema_diff_detects_added_removed_and_modified_tables() {
    let temp_dir = TempDir::new().expect("temp dir");
    let left_path = create_db(&temp_dir, "left.sqlite", "schema_left");
    let right_path = create_db(&temp_dir, "right.sqlite", "schema_right");

    let diff = diff_databases(&left_path, &right_path).expect("compute diff");
    assert_eq!(diff.schema.added_tables.len(), 1);
    assert_eq!(diff.schema.added_tables[0].name, "release_notes");
    assert_eq!(diff.schema.removed_tables.len(), 1);
    assert_eq!(diff.schema.removed_tables[0].name, "audit_log");
    assert_eq!(diff.schema.modified_tables.len(), 1);
    let users_diff = &diff.schema.modified_tables[0];
    assert_eq!(users_diff.table_name, "users");
    assert_eq!(users_diff.added_columns.len(), 1);
    assert_eq!(users_diff.added_columns[0].name, "status");
    assert_eq!(users_diff.modified_columns.len(), 2);
}

#[test]
fn data_diff_detects_added_removed_modified_rows_and_blobs() {
    let temp_dir = TempDir::new().expect("temp dir");
    let left_path = create_db(&temp_dir, "data-left.sqlite", "data_left");
    let right_path = create_db(&temp_dir, "data-right.sqlite", "data_right");

    let diff = diff_databases(&left_path, &right_path).expect("compute diff");
    assert_eq!(diff.data_diffs.len(), 1);
    let items = &diff.data_diffs[0];
    assert_eq!(items.table_name, "items");
    assert_eq!(items.stats.added, 1);
    assert_eq!(items.stats.removed, 1);
    assert_eq!(items.stats.modified, 1);
    assert_eq!(items.stats.unchanged, 1);
    assert_eq!(items.modified_rows[0].changes.len(), 3);
}

#[test]
fn rowid_fallback_emits_warning() {
    let temp_dir = TempDir::new().expect("temp dir");
    let left_path = create_db(&temp_dir, "rowid-left.sqlite", "rowid_left");
    let right_path = create_db(&temp_dir, "rowid-right.sqlite", "rowid_right");

    let diff = diff_databases(&left_path, &right_path).expect("compute diff");
    let notes = &diff.data_diffs[0];
    assert!(!notes.warnings.is_empty());
    assert_eq!(notes.stats.added, 1);
}

#[test]
fn sql_export_recreates_target_state() {
    let temp_dir = TempDir::new().expect("temp dir");
    let left_path = create_db(&temp_dir, "export-left.sqlite", "data_left");
    let right_path = create_db(&temp_dir, "export-right.sqlite", "data_right");
    let generated_path = temp_dir.path().join("generated.sqlite");

    std::fs::copy(&left_path, &generated_path).expect("copy left db");
    let diff = diff_databases(&left_path, &right_path).expect("compute diff");

    let generated = Connection::open(&generated_path).expect("open generated db");
    generated
        .execute_batch(&diff.sql_export)
        .expect("apply exported sql");

    let generated_summary = inspect_database(&generated_path).expect("inspect generated");
    let right_summary = inspect_database(&right_path).expect("inspect right");
    assert_eq!(generated_summary.tables, right_summary.tables);

    let generated_items =
        read_table_page(&generated_path, "items", &TableQuery::default()).expect("generated items");
    let right_items =
        read_table_page(&right_path, "items", &TableQuery::default()).expect("right items");
    assert_eq!(generated_items.rows, right_items.rows);
}

#[test]
fn sql_export_uses_rowid_for_removed_rows_without_primary_key() {
    let temp_dir = TempDir::new().expect("temp dir");
    let left_path = create_db(&temp_dir, "rowid-export-left.sqlite", "rowid_export_left");
    let right_path = create_db(&temp_dir, "rowid-export-right.sqlite", "rowid_export_right");
    let generated_path = temp_dir.path().join("rowid-export-generated.sqlite");

    std::fs::copy(&left_path, &generated_path).expect("copy left db");
    let diff = diff_databases(&left_path, &right_path).expect("compute diff");
    assert!(
        diff.sql_export
            .contains("DELETE FROM \"notes\" WHERE rowid = 20;"),
        "expected rowid delete in exported SQL, got:\n{}",
        diff.sql_export
    );

    let generated = Connection::open(&generated_path).expect("open generated db");
    generated
        .execute_batch(&diff.sql_export)
        .expect("apply exported sql");

    let generated_notes =
        read_table_page(&generated_path, "notes", &TableQuery::default()).expect("generated notes");
    let right_notes =
        read_table_page(&right_path, "notes", &TableQuery::default()).expect("right notes");
    assert_eq!(generated_notes.rows, right_notes.rows);
}

#[test]
fn snapshot_store_can_prepare_paths_for_diff_workflows() {
    let temp_dir = TempDir::new().expect("temp dir");
    let db_path = create_db(&temp_dir, "snapshot.sqlite", "snapshot_source");
    let store_root = temp_dir.path().join("snapshots");
    let store = SnapshotStore::new_in(&store_root).expect("create store");

    let snapshot = store
        .save_snapshot(&db_path, "first")
        .expect("save snapshot");
    let snapshot_path = store
        .load_snapshot_path(&snapshot.id)
        .expect("load snapshot path");

    assert!(Path::new(&snapshot_path).exists());
    let diff = diff_databases(&db_path, &snapshot_path).expect("diff snapshot");
    assert_eq!(diff.schema.added_tables.len(), 0);
    assert_eq!(diff.data_diffs[0].stats.added, 0);
}

#[test]
fn data_diff_treats_negative_zero_and_zero_as_equal() {
    let temp_dir = TempDir::new().expect("temp dir");
    let left_path = create_db_with_sql(
        &temp_dir,
        "float-left.sqlite",
        "
        CREATE TABLE measurements (id INTEGER PRIMARY KEY, reading REAL);
        INSERT INTO measurements (id, reading) VALUES (1, -0.0);
        ",
    );
    let right_path = create_db_with_sql(
        &temp_dir,
        "float-right.sqlite",
        "
        CREATE TABLE measurements (id INTEGER PRIMARY KEY, reading REAL);
        INSERT INTO measurements (id, reading) VALUES (1, 0.0);
        ",
    );

    let diff = diff_databases(&left_path, &right_path).expect("compute diff");
    let measurements = &diff.data_diffs[0];
    assert_eq!(measurements.stats.modified, 0);
    assert_eq!(measurements.stats.unchanged, 1);
}
