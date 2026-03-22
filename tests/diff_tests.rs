use std::path::Path;

use patchworks::db::differ::diff_databases;
use patchworks::db::inspector::{inspect_database, read_table_page};
use patchworks::db::snapshot::SnapshotStore;
use patchworks::db::types::{SortDirection, SqlValue, TableQuery, TableSort};
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
fn without_rowid_primary_key_fallback_still_produces_diff_and_export() {
    let temp_dir = TempDir::new().expect("temp dir");
    let left_path = create_db_with_sql(
        &temp_dir,
        "without-rowid-left.sqlite",
        "
        CREATE TABLE memberships (
            tenant_id INTEGER NOT NULL,
            user_id INTEGER NOT NULL,
            role TEXT NOT NULL,
            PRIMARY KEY (tenant_id, user_id)
        ) WITHOUT ROWID;
        INSERT INTO memberships (tenant_id, user_id, role) VALUES
            (1, 10, 'owner'),
            (1, 20, 'viewer');
        ",
    );
    let right_path = create_db_with_sql(
        &temp_dir,
        "without-rowid-right.sqlite",
        "
        CREATE TABLE memberships (
            account_id INTEGER NOT NULL,
            member_id INTEGER NOT NULL,
            role TEXT NOT NULL,
            PRIMARY KEY (account_id, member_id)
        ) WITHOUT ROWID;
        INSERT INTO memberships (account_id, member_id, role) VALUES
            (1, 10, 'owner'),
            (1, 30, 'editor');
        ",
    );
    let generated_path = temp_dir.path().join("without-rowid-generated.sqlite");

    std::fs::copy(&left_path, &generated_path).expect("copy left db");
    let diff = diff_databases(&left_path, &right_path).expect("compute diff");
    let memberships = diff
        .data_diffs
        .iter()
        .find(|table| table.table_name == "memberships")
        .expect("memberships diff");

    assert!(
        memberships
            .warnings
            .iter()
            .any(|warning| warning.contains("table-local row identity")),
        "expected fallback warning, got: {:?}",
        memberships.warnings
    );
    assert!(
        diff.sql_export
            .contains("CREATE TABLE __patchworks_new_memberships"),
        "expected temp-table rebuild in exported SQL, got:\n{}",
        diff.sql_export
    );

    let generated = Connection::open(&generated_path).expect("open generated db");
    generated
        .execute_batch(&diff.sql_export)
        .expect("apply exported sql");

    let generated_summary = inspect_database(&generated_path).expect("inspect generated");
    let right_summary = inspect_database(&right_path).expect("inspect right");
    let generated_table_shape = generated_summary
        .tables
        .iter()
        .map(|table| {
            (
                table.name.clone(),
                table.columns.clone(),
                table.row_count,
                table.primary_key.clone(),
            )
        })
        .collect::<Vec<_>>();
    let right_table_shape = right_summary
        .tables
        .iter()
        .map(|table| {
            (
                table.name.clone(),
                table.columns.clone(),
                table.row_count,
                table.primary_key.clone(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(generated_table_shape, right_table_shape);

    let generated_memberships =
        read_table_page(&generated_path, "memberships", &TableQuery::default())
            .expect("generated memberships");
    let right_memberships = read_table_page(&right_path, "memberships", &TableQuery::default())
        .expect("right memberships");
    assert_eq!(generated_memberships.rows, right_memberships.rows);
}

#[test]
fn sql_export_handles_foreign_keys_when_applied_with_foreign_keys_enabled() {
    let temp_dir = TempDir::new().expect("temp dir");
    let left_path = create_db_with_sql(
        &temp_dir,
        "fk-left.sqlite",
        "
        PRAGMA foreign_keys=ON;
        CREATE TABLE parents (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL
        );
        CREATE TABLE children (
            id INTEGER PRIMARY KEY,
            parent_id INTEGER NOT NULL REFERENCES parents(id),
            label TEXT NOT NULL
        );
        INSERT INTO parents (id, name) VALUES (1, 'alpha');
        INSERT INTO children (id, parent_id, label) VALUES (1, 1, 'keep');
        ",
    );
    let right_path = create_db_with_sql(
        &temp_dir,
        "fk-right.sqlite",
        "
        PRAGMA foreign_keys=ON;
        CREATE TABLE parents (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'active'
        );
        CREATE TABLE children (
            id INTEGER PRIMARY KEY,
            parent_id INTEGER NOT NULL REFERENCES parents(id),
            label TEXT NOT NULL
        );
        INSERT INTO parents (id, name, status) VALUES
            (1, 'alpha', 'active'),
            (2, 'beta', 'active');
        INSERT INTO children (id, parent_id, label) VALUES
            (1, 1, 'keep'),
            (2, 2, 'new child');
        ",
    );
    let generated_path = temp_dir.path().join("fk-generated.sqlite");

    std::fs::copy(&left_path, &generated_path).expect("copy left db");
    let diff = diff_databases(&left_path, &right_path).expect("compute diff");
    assert!(
        diff.sql_export
            .starts_with("PRAGMA foreign_keys=OFF;\nBEGIN TRANSACTION;"),
        "expected foreign-key guard in exported SQL, got:\n{}",
        diff.sql_export
    );

    let generated = Connection::open(&generated_path).expect("open generated db");
    generated
        .pragma_update(None, "foreign_keys", "ON")
        .expect("enable foreign keys");
    generated
        .execute_batch(&diff.sql_export)
        .expect("apply exported sql");

    let fk_enabled: i64 = generated
        .pragma_query_value(None, "foreign_keys", |row| row.get(0))
        .expect("read foreign_keys pragma");
    assert_eq!(fk_enabled, 1);

    let generated_summary = inspect_database(&generated_path).expect("inspect generated");
    let right_summary = inspect_database(&right_path).expect("inspect right");
    let generated_table_shape = generated_summary
        .tables
        .iter()
        .map(|table| {
            (
                table.name.clone(),
                table.columns.clone(),
                table.row_count,
                table.primary_key.clone(),
            )
        })
        .collect::<Vec<_>>();
    let right_table_shape = right_summary
        .tables
        .iter()
        .map(|table| {
            (
                table.name.clone(),
                table.columns.clone(),
                table.row_count,
                table.primary_key.clone(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(generated_table_shape, right_table_shape);

    let generated_parents = read_table_page(&generated_path, "parents", &TableQuery::default())
        .expect("generated parents");
    let right_parents =
        read_table_page(&right_path, "parents", &TableQuery::default()).expect("right parents");
    assert_eq!(generated_parents.rows, right_parents.rows);

    let generated_children = read_table_page(&generated_path, "children", &TableQuery::default())
        .expect("generated children");
    let right_children =
        read_table_page(&right_path, "children", &TableQuery::default()).expect("right children");
    assert_eq!(generated_children.rows, right_children.rows);

    let mut statement = generated
        .prepare("PRAGMA foreign_key_check")
        .expect("prepare foreign_key_check");
    let violations = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
            ))
        })
        .expect("query foreign_key_check")
        .collect::<std::result::Result<Vec<_>, _>>()
        .expect("collect foreign_key_check rows");
    assert!(
        violations.is_empty(),
        "foreign key violations: {violations:?}"
    );
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

#[test]
fn sorted_pagination_is_deterministic_for_duplicate_sort_values() {
    let temp_dir = TempDir::new().expect("temp dir");
    let db_path = create_db_with_sql(
        &temp_dir,
        "stable-sort.sqlite",
        "
        CREATE TABLE items (id INTEGER PRIMARY KEY, category TEXT NOT NULL, name TEXT NOT NULL);
        INSERT INTO items (id, category, name) VALUES
            (3, 'same', 'gamma'),
            (1, 'same', 'alpha'),
            (4, 'same', 'delta'),
            (2, 'same', 'beta');
        ",
    );

    let first_page = read_table_page(
        &db_path,
        "items",
        &TableQuery {
            page: 0,
            page_size: 2,
            sort: Some(TableSort {
                column: "category".to_owned(),
                direction: SortDirection::Asc,
            }),
        },
    )
    .expect("read first page");
    let second_page = read_table_page(
        &db_path,
        "items",
        &TableQuery {
            page: 1,
            page_size: 2,
            sort: Some(TableSort {
                column: "category".to_owned(),
                direction: SortDirection::Asc,
            }),
        },
    )
    .expect("read second page");

    let ids = first_page
        .rows
        .iter()
        .chain(second_page.rows.iter())
        .map(|row| match &row[0] {
            SqlValue::Integer(value) => *value,
            other => panic!("expected integer id, got {other:?}"),
        })
        .collect::<Vec<_>>();

    assert_eq!(ids, vec![1, 2, 3, 4]);
}

#[test]
fn sql_export_preserves_schema_objects_and_avoids_trigger_side_effects() {
    let temp_dir = TempDir::new().expect("temp dir");
    let left_path = create_db_with_sql(
        &temp_dir,
        "objects-left.sqlite",
        "
        CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT NOT NULL, qty INTEGER NOT NULL);
        CREATE TABLE audit (item_id INTEGER NOT NULL, action TEXT NOT NULL);
        CREATE INDEX idx_items_name ON items(name);
        CREATE TRIGGER items_audit_update AFTER UPDATE ON items
        BEGIN
            INSERT INTO audit (item_id, action) VALUES (NEW.id, 'updated');
        END;
        INSERT INTO items (id, name, qty) VALUES (1, 'alpha', 1), (2, 'beta', 1);
        ",
    );
    let right_path = create_db_with_sql(
        &temp_dir,
        "objects-right.sqlite",
        "
        CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT NOT NULL, qty INTEGER NOT NULL);
        CREATE TABLE audit (item_id INTEGER NOT NULL, action TEXT NOT NULL);
        CREATE INDEX idx_items_name ON items(name, qty);
        CREATE TRIGGER items_audit_update AFTER UPDATE OF name ON items
        BEGIN
            INSERT INTO audit (item_id, action) VALUES (NEW.id, 'renamed');
        END;
        INSERT INTO items (id, name, qty) VALUES (1, 'alpha', 1), (2, 'beta', 2);
        ",
    );
    let generated_path = temp_dir.path().join("objects-generated.sqlite");

    std::fs::copy(&left_path, &generated_path).expect("copy left db");
    let diff = diff_databases(&left_path, &right_path).expect("compute diff");
    assert_eq!(diff.schema.modified_indexes.len(), 1);
    assert_eq!(diff.schema.modified_triggers.len(), 1);

    let generated = Connection::open(&generated_path).expect("open generated db");
    generated
        .execute_batch(&diff.sql_export)
        .expect("apply exported sql");

    let generated_summary = inspect_database(&generated_path).expect("inspect generated");
    let right_summary = inspect_database(&right_path).expect("inspect right");
    assert_eq!(generated_summary.indexes, right_summary.indexes);
    assert_eq!(generated_summary.triggers, right_summary.triggers);

    let generated_items =
        read_table_page(&generated_path, "items", &TableQuery::default()).expect("generated items");
    let right_items =
        read_table_page(&right_path, "items", &TableQuery::default()).expect("right items");
    assert_eq!(generated_items.rows, right_items.rows);

    let generated_audit =
        read_table_page(&generated_path, "audit", &TableQuery::default()).expect("generated audit");
    let right_audit =
        read_table_page(&right_path, "audit", &TableQuery::default()).expect("right audit");
    assert_eq!(generated_audit.rows, right_audit.rows);
}
