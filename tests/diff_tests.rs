use std::io::BufWriter;
use std::path::Path;

use patchworks::db::differ::diff_databases;
use patchworks::db::inspector::{inspect_database, read_table_page};
use patchworks::db::snapshot::SnapshotStore;
use patchworks::db::types::{SortDirection, SqlValue, TableQuery, TableSort};
use patchworks::diff::export::write_export;
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

#[test]
fn wal_mode_databases_can_be_inspected_and_diffed() {
    let temp_dir = TempDir::new().expect("temp dir");
    let left_path = temp_dir.path().join("wal-left.sqlite");
    let right_path = temp_dir.path().join("wal-right.sqlite");

    let left_conn = Connection::open(&left_path).expect("open left");
    left_conn
        .execute_batch(
            "
            PRAGMA journal_mode=WAL;
            CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT NOT NULL);
            INSERT INTO items (id, name) VALUES (1, 'alpha'), (2, 'beta');
            ",
        )
        .expect("setup left wal db");
    drop(left_conn);

    let right_conn = Connection::open(&right_path).expect("open right");
    right_conn
        .execute_batch(
            "
            PRAGMA journal_mode=WAL;
            CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT NOT NULL);
            INSERT INTO items (id, name) VALUES (1, 'alpha'), (2, 'beta-changed'), (3, 'gamma');
            ",
        )
        .expect("setup right wal db");
    drop(right_conn);

    let left_summary = inspect_database(&left_path).expect("inspect wal left");
    assert_eq!(left_summary.tables.len(), 1);
    assert_eq!(left_summary.tables[0].row_count, 2);

    let diff = diff_databases(&left_path, &right_path).expect("diff wal databases");
    let items = &diff.data_diffs[0];
    assert_eq!(items.stats.modified, 1);
    assert_eq!(items.stats.added, 1);
    assert_eq!(items.stats.unchanged, 1);

    let generated_path = temp_dir.path().join("wal-generated.sqlite");
    std::fs::copy(&left_path, &generated_path).expect("copy left db");
    // Also copy the WAL file if it exists so the copy is complete
    let left_wal = left_path.with_extension("sqlite-wal");
    let generated_wal = generated_path.with_extension("sqlite-wal");
    if left_wal.exists() {
        std::fs::copy(&left_wal, &generated_wal).expect("copy wal file");
    }

    let generated = Connection::open(&generated_path).expect("open generated db");
    generated
        .execute_batch(&diff.sql_export)
        .expect("apply exported sql to wal db");

    let generated_items =
        read_table_page(&generated_path, "items", &TableQuery::default()).expect("generated items");
    let right_items =
        read_table_page(&right_path, "items", &TableQuery::default()).expect("right items");
    assert_eq!(generated_items.rows, right_items.rows);
}

#[test]
fn streaming_export_matches_in_memory_export() {
    let temp_dir = TempDir::new().expect("temp dir");
    let left_path = create_db(&temp_dir, "stream-left.sqlite", "data_left");
    let right_path = create_db(&temp_dir, "stream-right.sqlite", "data_right");

    let diff = diff_databases(&left_path, &right_path).expect("compute diff");

    let mut streamed = Vec::new();
    write_export(
        &mut streamed,
        &std::path::PathBuf::from(&diff.right.path),
        &diff.left,
        &diff.right,
        &diff.schema,
        &diff.data_diffs,
    )
    .expect("streaming export");
    let streamed_sql = String::from_utf8(streamed).expect("valid utf8");

    assert_eq!(
        streamed_sql, diff.sql_export,
        "streaming export should produce identical output to in-memory export"
    );
}

#[test]
fn streaming_export_to_file_produces_valid_migration() {
    let temp_dir = TempDir::new().expect("temp dir");
    let left_path = create_db(&temp_dir, "file-export-left.sqlite", "data_left");
    let right_path = create_db(&temp_dir, "file-export-right.sqlite", "data_right");
    let export_path = temp_dir.path().join("migration.sql");
    let generated_path = temp_dir.path().join("file-export-generated.sqlite");

    let diff = diff_databases(&left_path, &right_path).expect("compute diff");

    // Write export to a file via the streaming API
    let file = std::fs::File::create(&export_path).expect("create export file");
    let mut writer = BufWriter::new(file);
    write_export(
        &mut writer,
        &std::path::PathBuf::from(&diff.right.path),
        &diff.left,
        &diff.right,
        &diff.schema,
        &diff.data_diffs,
    )
    .expect("write export to file");
    drop(writer);

    // Read it back and apply to a copy of the left database
    let exported_sql = std::fs::read_to_string(&export_path).expect("read export file");
    std::fs::copy(&left_path, &generated_path).expect("copy left db");

    let generated = Connection::open(&generated_path).expect("open generated db");
    generated
        .execute_batch(&exported_sql)
        .expect("apply file-exported sql");

    let generated_items =
        read_table_page(&generated_path, "items", &TableQuery::default()).expect("generated items");
    let right_items =
        read_table_page(&right_path, "items", &TableQuery::default()).expect("right items");
    assert_eq!(generated_items.rows, right_items.rows);
}

#[test]
fn large_table_export_streams_without_full_materialization() {
    let temp_dir = TempDir::new().expect("temp dir");
    let left_path = temp_dir.path().join("large-left.sqlite");
    let right_path = temp_dir.path().join("large-right.sqlite");

    // Create a table with enough rows to be meaningful but not slow
    let left_conn = Connection::open(&left_path).expect("open left");
    left_conn
        .execute_batch("CREATE TABLE records (id INTEGER PRIMARY KEY, payload TEXT NOT NULL);")
        .expect("create left table");
    {
        let mut insert = left_conn
            .prepare("INSERT INTO records (id, payload) VALUES (?, ?)")
            .expect("prepare insert");
        for i in 1..=5000 {
            insert
                .execute(rusqlite::params![i, format!("left-payload-{i}")])
                .expect("insert row");
        }
    }
    drop(left_conn);

    let right_conn = Connection::open(&right_path).expect("open right");
    right_conn
        .execute_batch("CREATE TABLE records (id INTEGER PRIMARY KEY, payload TEXT NOT NULL);")
        .expect("create right table");
    {
        let mut insert = right_conn
            .prepare("INSERT INTO records (id, payload) VALUES (?, ?)")
            .expect("prepare insert");
        for i in 1..=5000 {
            let payload = if i % 100 == 0 {
                format!("modified-payload-{i}")
            } else {
                format!("left-payload-{i}")
            };
            insert
                .execute(rusqlite::params![i, payload])
                .expect("insert row");
        }
        // Add 100 new rows
        for i in 5001..=5100 {
            insert
                .execute(rusqlite::params![i, format!("new-payload-{i}")])
                .expect("insert new row");
        }
    }
    drop(right_conn);

    let diff = diff_databases(&left_path, &right_path).expect("diff large tables");
    let records = &diff.data_diffs[0];
    assert_eq!(records.stats.modified, 50); // every 100th row out of 5000
    assert_eq!(records.stats.added, 100);
    assert_eq!(records.stats.unchanged, 4950);

    // Streaming export to file should work
    let export_path = temp_dir.path().join("large-migration.sql");
    let file = std::fs::File::create(&export_path).expect("create export file");
    let mut writer = BufWriter::new(file);
    write_export(
        &mut writer,
        &right_path,
        &diff.left,
        &diff.right,
        &diff.schema,
        &diff.data_diffs,
    )
    .expect("stream large export");
    drop(writer);

    // Apply and verify
    let generated_path = temp_dir.path().join("large-generated.sqlite");
    std::fs::copy(&left_path, &generated_path).expect("copy left db");
    let exported_sql = std::fs::read_to_string(&export_path).expect("read export");
    let generated = Connection::open(&generated_path).expect("open generated db");
    generated
        .execute_batch(&exported_sql)
        .expect("apply large export");

    let gen_summary = inspect_database(&generated_path).expect("inspect generated");
    let right_summary = inspect_database(&right_path).expect("inspect right");
    assert_eq!(
        gen_summary.tables[0].row_count,
        right_summary.tables[0].row_count
    );
}

#[test]
fn diff_handles_empty_databases() {
    let temp_dir = TempDir::new().expect("temp dir");
    let left_path = create_db_with_sql(&temp_dir, "empty-left.sqlite", "");
    let right_path = create_db_with_sql(&temp_dir, "empty-right.sqlite", "");

    let diff = diff_databases(&left_path, &right_path).expect("diff empty databases");
    assert!(diff.schema.added_tables.is_empty());
    assert!(diff.schema.removed_tables.is_empty());
    assert!(diff.data_diffs.is_empty());
    assert!(diff.sql_export.contains("PRAGMA foreign_keys=OFF;"));
    assert!(diff.sql_export.contains("COMMIT;"));
}

#[test]
fn diff_handles_table_added_from_empty() {
    let temp_dir = TempDir::new().expect("temp dir");
    let left_path = create_db_with_sql(&temp_dir, "add-from-empty-left.sqlite", "");
    let right_path = create_db_with_sql(
        &temp_dir,
        "add-from-empty-right.sqlite",
        "
        CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT NOT NULL);
        INSERT INTO items (id, name) VALUES (1, 'first'), (2, 'second');
        ",
    );

    let diff = diff_databases(&left_path, &right_path).expect("diff add from empty");
    assert_eq!(diff.schema.added_tables.len(), 1);
    assert_eq!(diff.schema.added_tables[0].name, "items");

    let generated_path = temp_dir.path().join("add-from-empty-generated.sqlite");
    std::fs::copy(&left_path, &generated_path).expect("copy left db");
    let generated = Connection::open(&generated_path).expect("open generated");
    generated
        .execute_batch(&diff.sql_export)
        .expect("apply export");

    let gen_items =
        read_table_page(&generated_path, "items", &TableQuery::default()).expect("gen items");
    let right_items =
        read_table_page(&right_path, "items", &TableQuery::default()).expect("right items");
    assert_eq!(gen_items.rows, right_items.rows);
}
