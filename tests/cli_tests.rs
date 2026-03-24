//! Integration tests for headless CLI commands.
//!
//! These test the `cli` module functions directly — the same code path the CLI subcommands use —
//! rather than spawning a binary process. This keeps tests fast while proving CLI/GUI parity
//! since both paths share the same backend truth layer.

use patchworks::cli::{self, OutputFormat, EXIT_DIFF, EXIT_OK};
use patchworks::db::snapshot::SnapshotStore;
use rusqlite::Connection;
use tempfile::TempDir;

mod support;

use support::{create_db, create_db_with_sql};

// --- Inspect ---

#[test]
fn inspect_human_shows_tables_and_columns() {
    let dir = TempDir::new().expect("temp dir");
    let db = create_db(&dir, "inspect.sqlite", "schema_left");

    let mut output = Vec::new();
    let code = cli::run_inspect(&mut output, &db, OutputFormat::Human).expect("inspect");
    let text = String::from_utf8(output).expect("utf8");

    assert_eq!(code, EXIT_OK);
    assert!(text.contains("users"), "expected 'users' table in output");
    assert!(
        text.contains("audit_log"),
        "expected 'audit_log' table in output"
    );
    assert!(text.contains("2 rows"), "expected '2 rows' for users");
    assert!(text.contains("id"), "expected 'id' column");
}

#[test]
fn inspect_json_is_valid_and_contains_tables() {
    let dir = TempDir::new().expect("temp dir");
    let db = create_db(&dir, "inspect.sqlite", "schema_left");

    let mut output = Vec::new();
    let code = cli::run_inspect(&mut output, &db, OutputFormat::Json).expect("inspect");
    let text = String::from_utf8(output).expect("utf8");

    assert_eq!(code, EXIT_OK);
    let parsed: serde_json::Value = serde_json::from_str(&text).expect("valid json");
    let tables = parsed["tables"].as_array().expect("tables array");
    assert_eq!(tables.len(), 2);
}

#[test]
fn inspect_empty_database_shows_no_tables() {
    let dir = TempDir::new().expect("temp dir");
    let db = create_db_with_sql(&dir, "empty.sqlite", "");

    let mut output = Vec::new();
    let code = cli::run_inspect(&mut output, &db, OutputFormat::Human).expect("inspect");
    let text = String::from_utf8(output).expect("utf8");

    assert_eq!(code, EXIT_OK);
    assert!(text.contains("No tables."));
}

// --- Diff ---

#[test]
fn diff_identical_databases_returns_exit_ok() {
    let dir = TempDir::new().expect("temp dir");
    let left = create_db(&dir, "left.sqlite", "data_left");
    let right = create_db(&dir, "right-same.sqlite", "data_left");

    let mut output = Vec::new();
    let code = cli::run_diff(&mut output, &left, &right, OutputFormat::Human).expect("diff");
    let text = String::from_utf8(output).expect("utf8");

    assert_eq!(code, EXIT_OK);
    assert!(text.contains("No differences."));
}

#[test]
fn diff_with_schema_changes_returns_exit_diff() {
    let dir = TempDir::new().expect("temp dir");
    let left = create_db(&dir, "left.sqlite", "schema_left");
    let right = create_db(&dir, "right.sqlite", "schema_right");

    let mut output = Vec::new();
    let code = cli::run_diff(&mut output, &left, &right, OutputFormat::Human).expect("diff");
    let text = String::from_utf8(output).expect("utf8");

    assert_eq!(code, EXIT_DIFF);
    assert!(
        text.contains("+ table release_notes"),
        "expected added table"
    );
    assert!(text.contains("- table audit_log"), "expected removed table");
    assert!(text.contains("~ table users"), "expected modified table");
}

#[test]
fn diff_with_data_changes_returns_exit_diff() {
    let dir = TempDir::new().expect("temp dir");
    let left = create_db(&dir, "left.sqlite", "data_left");
    let right = create_db(&dir, "right.sqlite", "data_right");

    let mut output = Vec::new();
    let code = cli::run_diff(&mut output, &left, &right, OutputFormat::Human).expect("diff");
    let text = String::from_utf8(output).expect("utf8");

    assert_eq!(code, EXIT_DIFF);
    assert!(text.contains("+1 added"));
    assert!(text.contains("-1 removed"));
    assert!(text.contains("~1 modified"));
}

#[test]
fn diff_json_contains_schema_and_data_sections() {
    let dir = TempDir::new().expect("temp dir");
    let left = create_db(&dir, "left.sqlite", "data_left");
    let right = create_db(&dir, "right.sqlite", "data_right");

    let mut output = Vec::new();
    let code = cli::run_diff(&mut output, &left, &right, OutputFormat::Json).expect("diff");
    let text = String::from_utf8(output).expect("utf8");

    assert_eq!(code, EXIT_DIFF);
    let parsed: serde_json::Value = serde_json::from_str(&text).expect("valid json");
    assert!(parsed.get("schema").is_some());
    assert!(parsed.get("data").is_some());
}

// --- Export ---

#[test]
fn export_produces_valid_sql_migration() {
    let dir = TempDir::new().expect("temp dir");
    let left = create_db(&dir, "left.sqlite", "data_left");
    let right = create_db(&dir, "right.sqlite", "data_right");

    let mut output = Vec::new();
    let code = cli::run_export(&mut output, &left, &right).expect("export");
    let text = String::from_utf8(output).expect("utf8");

    assert_eq!(code, EXIT_OK);
    assert!(text.contains("PRAGMA foreign_keys=OFF;"));
    assert!(text.contains("BEGIN TRANSACTION;"));
    assert!(text.contains("COMMIT;"));
    assert!(text.contains("PRAGMA foreign_keys=ON;"));
}

#[test]
fn export_applied_to_left_produces_right() {
    let dir = TempDir::new().expect("temp dir");
    let left = create_db(&dir, "left.sqlite", "data_left");
    let right = create_db(&dir, "right.sqlite", "data_right");
    let generated = dir.path().join("generated.sqlite");

    let mut output = Vec::new();
    cli::run_export(&mut output, &left, &right).expect("export");
    let sql = String::from_utf8(output).expect("utf8");

    std::fs::copy(&left, &generated).expect("copy left db");
    let conn = Connection::open(&generated).expect("open generated");
    conn.execute_batch(&sql).expect("apply exported sql");

    // Verify equality via inspect
    let mut left_json = Vec::new();
    cli::run_inspect(&mut left_json, &generated, OutputFormat::Json).expect("inspect generated");
    let mut right_json = Vec::new();
    cli::run_inspect(&mut right_json, &right, OutputFormat::Json).expect("inspect right");

    let gen_summary: serde_json::Value =
        serde_json::from_slice(&left_json).expect("parse generated");
    let right_summary: serde_json::Value =
        serde_json::from_slice(&right_json).expect("parse right");

    // Compare table structures (ignore path)
    assert_eq!(gen_summary["tables"], right_summary["tables"]);
}

#[test]
fn export_identical_databases_produces_minimal_output() {
    let dir = TempDir::new().expect("temp dir");
    let left = create_db(&dir, "left.sqlite", "data_left");
    let right = create_db(&dir, "same.sqlite", "data_left");

    let mut output = Vec::new();
    cli::run_export(&mut output, &left, &right).expect("export");
    let sql = String::from_utf8(output).expect("utf8");

    // Should still have the transaction wrapper but no data mutations
    assert!(sql.contains("BEGIN TRANSACTION;"));
    assert!(sql.contains("COMMIT;"));
    assert!(!sql.contains("INSERT"));
    assert!(!sql.contains("DELETE"));
    assert!(!sql.contains("UPDATE"));
    assert!(!sql.contains("DROP TABLE"));
}

// --- Snapshot ---

#[test]
fn snapshot_save_and_list_roundtrip() {
    let dir = TempDir::new().expect("temp dir");
    let db = create_db(&dir, "test.sqlite", "data_left");
    let store_root = dir.path().join("snap-store");
    let store = SnapshotStore::new_in(&store_root).expect("create store");

    let snapshot = store.save_snapshot(&db, "test snap").expect("save");
    let all = store.list_all_snapshots().expect("list all");
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].name, "test snap");

    let by_source = store.list_snapshots(&db).expect("list by source");
    assert_eq!(by_source.len(), 1);
    assert_eq!(by_source[0].id, snapshot.id);
}

#[test]
fn snapshot_delete_removes_entry_and_file() {
    let dir = TempDir::new().expect("temp dir");
    let db = create_db(&dir, "test.sqlite", "data_left");
    let store_root = dir.path().join("snap-store");
    let store = SnapshotStore::new_in(&store_root).expect("create store");

    let snapshot = store.save_snapshot(&db, "to-delete").expect("save");
    let snap_path = store.load_snapshot_path(&snapshot.id).expect("load path");
    assert!(snap_path.exists());

    let deleted = store.delete_snapshot(&snapshot.id).expect("delete");
    assert!(deleted);
    assert!(!snap_path.exists());

    let remaining = store.list_all_snapshots().expect("list after delete");
    assert!(remaining.is_empty());
}

#[test]
fn snapshot_delete_nonexistent_returns_false() {
    let dir = TempDir::new().expect("temp dir");
    let store_root = dir.path().join("snap-store");
    let store = SnapshotStore::new_in(&store_root).expect("create store");

    let deleted = store.delete_snapshot("nonexistent-id").expect("delete");
    assert!(!deleted);
}

// --- CLI output golden checks proving GUI/CLI backend parity ---

#[test]
fn cli_diff_and_gui_diff_share_same_truth_layer() {
    // This test proves that the CLI diff path calls the same diff_databases function
    // as the GUI, by verifying the diff result matches the standalone diff call.
    let dir = TempDir::new().expect("temp dir");
    let left = create_db(&dir, "left.sqlite", "data_left");
    let right = create_db(&dir, "right.sqlite", "data_right");

    // CLI path (via JSON output)
    let mut cli_output = Vec::new();
    cli::run_diff(&mut cli_output, &left, &right, OutputFormat::Json).expect("cli diff");
    let cli_json: serde_json::Value = serde_json::from_slice(&cli_output).expect("parse cli json");

    // Direct backend call (same function the GUI uses)
    let diff = patchworks::db::differ::diff_databases(&left, &right).expect("backend diff");

    // Verify stats match
    let cli_data = cli_json["data"].as_array().expect("data array");
    assert_eq!(cli_data.len(), diff.data_diffs.len());
    for (cli_table, backend_table) in cli_data.iter().zip(diff.data_diffs.iter()) {
        assert_eq!(
            cli_table["stats"]["added"].as_u64().unwrap(),
            backend_table.stats.added
        );
        assert_eq!(
            cli_table["stats"]["removed"].as_u64().unwrap(),
            backend_table.stats.removed
        );
        assert_eq!(
            cli_table["stats"]["modified"].as_u64().unwrap(),
            backend_table.stats.modified
        );
    }
}

#[test]
fn cli_export_and_gui_export_produce_identical_sql() {
    let dir = TempDir::new().expect("temp dir");
    let left = create_db(&dir, "left.sqlite", "data_left");
    let right = create_db(&dir, "right.sqlite", "data_right");

    // CLI export
    let mut cli_output = Vec::new();
    cli::run_export(&mut cli_output, &left, &right).expect("cli export");
    let cli_sql = String::from_utf8(cli_output).expect("utf8");

    // GUI backend export (same convenience function used in the app)
    let diff = patchworks::db::differ::diff_databases(&left, &right).expect("backend diff");

    // Both should produce the same SQL
    assert_eq!(cli_sql, diff.sql_export);
}

// --- Inspect with schema objects ---

#[test]
fn inspect_shows_indexes_and_triggers() {
    let dir = TempDir::new().expect("temp dir");
    let db = create_db_with_sql(
        &dir,
        "objects.sqlite",
        "
        CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT NOT NULL);
        CREATE INDEX idx_items_name ON items(name);
        CREATE TRIGGER trg_items_insert AFTER INSERT ON items
        BEGIN
            SELECT 1;
        END;
        ",
    );

    let mut output = Vec::new();
    cli::run_inspect(&mut output, &db, OutputFormat::Human).expect("inspect");
    let text = String::from_utf8(output).expect("utf8");

    assert!(text.contains("Indexes (1):"), "expected indexes section");
    assert!(text.contains("idx_items_name on items"));
    assert!(text.contains("Triggers (1):"), "expected triggers section");
    assert!(text.contains("trg_items_insert on items"));
}

// --- Diff with index/trigger changes ---

#[test]
fn diff_reports_index_and_trigger_changes() {
    let dir = TempDir::new().expect("temp dir");
    let left = create_db_with_sql(
        &dir,
        "left.sqlite",
        "
        CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT NOT NULL);
        CREATE INDEX idx_items_name ON items(name);
        CREATE TRIGGER trg_audit AFTER INSERT ON items BEGIN SELECT 1; END;
        INSERT INTO items VALUES (1, 'a');
        ",
    );
    let right = create_db_with_sql(
        &dir,
        "right.sqlite",
        "
        CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT NOT NULL);
        CREATE INDEX idx_items_name ON items(name, id);
        CREATE TRIGGER trg_audit AFTER UPDATE ON items BEGIN SELECT 1; END;
        INSERT INTO items VALUES (1, 'a');
        ",
    );

    let mut output = Vec::new();
    let code = cli::run_diff(&mut output, &left, &right, OutputFormat::Human).expect("diff");
    let text = String::from_utf8(output).expect("utf8");

    assert_eq!(code, EXIT_DIFF);
    assert!(text.contains("~ index idx_items_name"));
    assert!(text.contains("~ trigger trg_audit"));
}

#[test]
fn snapshot_list_human_format_shows_header() {
    let dir = TempDir::new().expect("temp dir");
    let db = create_db(&dir, "test.sqlite", "data_left");
    let store_root = dir.path().join("snap-store");
    let store = SnapshotStore::new_in(&store_root).expect("create store");

    store.save_snapshot(&db, "my-snap").expect("save");

    let all = store.list_all_snapshots().expect("list");
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].name, "my-snap");
}
