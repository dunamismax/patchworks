//! Integration tests for Phase 8: migration workflow management.

use std::path::PathBuf;

use patchworks::db::migration::{MigrationStore, NewMigration};
use patchworks::diff::migration::{
    apply_migration, collect_affected_tables, generate_down_sql, generate_up_sql,
    squash_migrations, validate_migration, validate_rollback,
};
use rusqlite::Connection;
use tempfile::TempDir;

// --- Test helpers ---

fn create_db(dir: &TempDir, name: &str, sql: &str) -> PathBuf {
    let path = dir.path().join(name);
    let conn = Connection::open(&path).expect("create db");
    conn.execute_batch(sql).expect("execute sql");
    path
}

fn read_all_rows(path: &PathBuf, table: &str) -> Vec<Vec<String>> {
    let conn = Connection::open(path).expect("open db");
    let mut stmt = conn
        .prepare(&format!("SELECT * FROM {} ORDER BY 1", table))
        .expect("prepare");
    let col_count = stmt.column_count();
    let rows = stmt
        .query_map([], |row| {
            let mut values = Vec::new();
            for i in 0..col_count {
                let val: String = row.get::<_, rusqlite::types::Value>(i).map(|v| match v {
                    rusqlite::types::Value::Null => "NULL".to_string(),
                    rusqlite::types::Value::Integer(i) => i.to_string(),
                    rusqlite::types::Value::Real(f) => f.to_string(),
                    rusqlite::types::Value::Text(s) => s,
                    rusqlite::types::Value::Blob(b) => format!("[blob:{}]", b.len()),
                })?;
                values.push(val);
            }
            Ok(values)
        })
        .expect("query");
    rows.map(|r| r.expect("row")).collect()
}

// --- Migration generation tests ---

#[test]
fn generate_up_sql_produces_valid_migration() {
    let dir = TempDir::new().expect("temp dir");
    let left = create_db(
        &dir,
        "left.db",
        "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);
         INSERT INTO items VALUES (1, 'a'), (2, 'b');",
    );
    let right = create_db(
        &dir,
        "right.db",
        "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);
         INSERT INTO items VALUES (1, 'a'), (2, 'changed'), (3, 'new');",
    );

    let sql = generate_up_sql(&left, &right).expect("generate up sql");
    assert!(sql.contains("BEGIN TRANSACTION;"));
    assert!(sql.contains("COMMIT;"));
    // Should have an UPDATE and an INSERT
    assert!(sql.contains("UPDATE"));
    assert!(sql.contains("INSERT"));
}

#[test]
fn generate_up_sql_handles_schema_changes() {
    let dir = TempDir::new().expect("temp dir");
    let left = create_db(
        &dir,
        "left.db",
        "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);",
    );
    let right = create_db(
        &dir,
        "right.db",
        "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);
         CREATE TABLE orders (id INTEGER PRIMARY KEY, item_id INTEGER);",
    );

    let sql = generate_up_sql(&left, &right).expect("generate up sql");
    assert!(sql.contains("CREATE TABLE"));
    assert!(sql.contains("orders"));
}

#[test]
fn generate_down_sql_produces_reverse_migration() {
    let dir = TempDir::new().expect("temp dir");
    let left = create_db(
        &dir,
        "left.db",
        "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);
         INSERT INTO items VALUES (1, 'a');",
    );
    let right = create_db(
        &dir,
        "right.db",
        "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);
         INSERT INTO items VALUES (1, 'a'), (2, 'b');",
    );

    let down = generate_down_sql(&left, &right).expect("generate down sql");
    assert!(down.is_some(), "should have rollback SQL");
    let down_sql = down.unwrap();
    // The reverse of adding row 2 should delete it
    assert!(down_sql.contains("DELETE FROM"));
}

#[test]
fn generate_down_sql_returns_none_for_identical_databases() {
    let dir = TempDir::new().expect("temp dir");
    let left = create_db(
        &dir,
        "left.db",
        "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);
         INSERT INTO items VALUES (1, 'a');",
    );
    let right = create_db(
        &dir,
        "right.db",
        "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);
         INSERT INTO items VALUES (1, 'a');",
    );

    let down = generate_down_sql(&left, &right).expect("generate down sql");
    assert!(down.is_none(), "identical dbs should produce no rollback");
}

// --- Migration validation tests ---

#[test]
fn validate_migration_succeeds_for_correct_migration() {
    let dir = TempDir::new().expect("temp dir");
    let left = create_db(
        &dir,
        "left.db",
        "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);
         INSERT INTO items VALUES (1, 'a');",
    );
    let right = create_db(
        &dir,
        "right.db",
        "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);
         INSERT INTO items VALUES (1, 'a'), (2, 'b');",
    );

    let up_sql = generate_up_sql(&left, &right).expect("generate");
    let validation = validate_migration(&left, &right, &up_sql).expect("validate");

    assert!(validation.success, "migration should apply cleanly");
    assert!(
        validation.matches_target,
        "result should match target: differing tables: {:?}",
        validation.differing_tables
    );
    assert!(validation.error.is_none());
    assert!(validation.differing_tables.is_empty());
}

#[test]
fn validate_migration_detects_bad_sql() {
    let dir = TempDir::new().expect("temp dir");
    let left = create_db(
        &dir,
        "left.db",
        "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);",
    );
    let right = create_db(
        &dir,
        "right.db",
        "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);",
    );

    let validation = validate_migration(&left, &right, "INVALID SQL GARBAGE;").expect("validate");
    assert!(!validation.success, "invalid SQL should fail");
    assert!(validation.error.is_some());
}

#[test]
fn validate_migration_detects_mismatch() {
    let dir = TempDir::new().expect("temp dir");
    let left = create_db(
        &dir,
        "left.db",
        "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);
         INSERT INTO items VALUES (1, 'a');",
    );
    let right = create_db(
        &dir,
        "right.db",
        "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);
         INSERT INTO items VALUES (1, 'a'), (2, 'b'), (3, 'c');",
    );

    // Apply a partial migration that only adds one row instead of two
    let partial_sql = "BEGIN TRANSACTION;\nINSERT INTO items VALUES (2, 'b');\nCOMMIT;";
    let validation = validate_migration(&left, &right, partial_sql).expect("validate");

    assert!(validation.success, "SQL should apply cleanly");
    assert!(
        !validation.matches_target,
        "partial migration should not match target"
    );
    assert!(!validation.differing_tables.is_empty());
}

// --- Rollback validation tests ---

#[test]
fn validate_rollback_roundtrips_to_original() {
    let dir = TempDir::new().expect("temp dir");
    let left = create_db(
        &dir,
        "left.db",
        "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);
         INSERT INTO items VALUES (1, 'a');",
    );
    let right = create_db(
        &dir,
        "right.db",
        "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);
         INSERT INTO items VALUES (1, 'a'), (2, 'b');",
    );

    let up_sql = generate_up_sql(&left, &right).expect("generate up");
    let down_sql = generate_down_sql(&left, &right)
        .expect("generate down")
        .expect("should be reversible");

    let validation = validate_rollback(&left, &up_sql, &down_sql).expect("validate");
    assert!(validation.success, "rollback should apply cleanly");
    assert!(
        validation.matches_target,
        "rollback result should match original source: differing: {:?}",
        validation.differing_tables
    );
}

#[test]
fn validate_rollback_detects_bad_down_sql() {
    let dir = TempDir::new().expect("temp dir");
    let left = create_db(
        &dir,
        "left.db",
        "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);",
    );

    let up_sql = "SELECT 1;";
    let down_sql = "INVALID DOWN SQL;";
    let validation = validate_rollback(&left, up_sql, down_sql).expect("validate");
    assert!(!validation.success);
    assert!(validation.error.is_some());
}

// --- Migration store tests ---

#[test]
fn migration_store_saves_and_lists_migrations() {
    let dir = TempDir::new().expect("temp dir");
    let store = MigrationStore::new_in(dir.path().join("store")).expect("create store");

    let m1 = store
        .save_migration(&NewMigration {
            name: "add-orders",
            up_sql: "CREATE TABLE orders (id INTEGER PRIMARY KEY);",
            down_sql: Some("DROP TABLE orders;"),
            source_path: "/tmp/left.db",
            target_path: "/tmp/right.db",
            affected_tables: &["orders".to_owned()],
            description: Some("Add orders table"),
        })
        .expect("save m1");

    let m2 = store
        .save_migration(&NewMigration {
            name: "add-users",
            up_sql: "CREATE TABLE users (id INTEGER PRIMARY KEY);",
            down_sql: None,
            source_path: "/tmp/left.db",
            target_path: "/tmp/right.db",
            affected_tables: &["users".to_owned()],
            description: None,
        })
        .expect("save m2");

    assert_eq!(m1.sequence, 1);
    assert_eq!(m2.sequence, 2);

    let list = store.list_migrations().expect("list");
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].name, "add-orders");
    assert_eq!(list[1].name, "add-users");
}

#[test]
fn migration_store_get_and_delete() {
    let dir = TempDir::new().expect("temp dir");
    let store = MigrationStore::new_in(dir.path().join("store")).expect("create store");

    let m = store
        .save_migration(&NewMigration {
            name: "test-migration",
            up_sql: "SELECT 1;",
            down_sql: None,
            source_path: "/tmp/a.db",
            target_path: "/tmp/b.db",
            affected_tables: &[],
            description: None,
        })
        .expect("save");

    let fetched = store.get_migration(&m.id).expect("get");
    assert_eq!(fetched.name, "test-migration");

    assert!(store.delete_migration(&m.id).expect("delete"));
    assert!(store.get_migration(&m.id).is_err());
}

#[test]
fn migration_store_mark_validated() {
    let dir = TempDir::new().expect("temp dir");
    let store = MigrationStore::new_in(dir.path().join("store")).expect("create store");

    let m = store
        .save_migration(&NewMigration {
            name: "to-validate",
            up_sql: "SELECT 1;",
            down_sql: None,
            source_path: "/a",
            target_path: "/b",
            affected_tables: &[],
            description: None,
        })
        .expect("save");

    assert!(!m.validated);
    store.mark_validated(&m.id).expect("mark");
    let fetched = store.get_migration(&m.id).expect("get");
    assert!(fetched.validated);
}

#[test]
fn migration_store_chain_summary() {
    let dir = TempDir::new().expect("temp dir");
    let store = MigrationStore::new_in(dir.path().join("store")).expect("create store");

    store
        .save_migration(&NewMigration {
            name: "m1",
            up_sql: "SELECT 1;",
            down_sql: Some("SELECT 2;"),
            source_path: "/a",
            target_path: "/b",
            affected_tables: &["users".to_owned()],
            description: None,
        })
        .expect("save m1");
    store
        .save_migration(&NewMigration {
            name: "m2",
            up_sql: "SELECT 3;",
            down_sql: None,
            source_path: "/a",
            target_path: "/b",
            affected_tables: &["users".to_owned(), "orders".to_owned()],
            description: None,
        })
        .expect("save m2");

    let summary = store.chain_summary().expect("summary");
    assert_eq!(summary.total_migrations, 2);
    assert_eq!(summary.reversible_count, 1);
    assert_eq!(summary.first_sequence, Some(1));
    assert_eq!(summary.last_sequence, Some(2));
    assert!(summary.all_affected_tables.contains(&"users".to_owned()));
    assert!(summary.all_affected_tables.contains(&"orders".to_owned()));
}

#[test]
fn migration_store_conflict_detection() {
    let dir = TempDir::new().expect("temp dir");
    let store = MigrationStore::new_in(dir.path().join("store")).expect("create store");

    store
        .save_migration(&NewMigration {
            name: "m1",
            up_sql: "ALTER TABLE users ADD COLUMN email TEXT;",
            down_sql: None,
            source_path: "/a",
            target_path: "/b",
            affected_tables: &["users".to_owned()],
            description: None,
        })
        .expect("save");
    store
        .save_migration(&NewMigration {
            name: "m2",
            up_sql: "ALTER TABLE users ADD COLUMN phone TEXT;",
            down_sql: None,
            source_path: "/a",
            target_path: "/b",
            affected_tables: &["users".to_owned()],
            description: None,
        })
        .expect("save");
    store
        .save_migration(&NewMigration {
            name: "m3",
            up_sql: "CREATE TABLE orders (id INTEGER PRIMARY KEY);",
            down_sql: None,
            source_path: "/a",
            target_path: "/b",
            affected_tables: &["orders".to_owned()],
            description: None,
        })
        .expect("save");

    let conflicts = store.detect_conflicts().expect("detect");
    // m1 and m2 both touch "users", so there should be one conflict
    assert_eq!(conflicts.len(), 1);
    assert!(conflicts[0]
        .overlapping_tables
        .contains(&"users".to_owned()));
    assert!(conflicts[0].description.contains("users"));
}

#[test]
fn migration_store_no_conflicts_for_disjoint_migrations() {
    let dir = TempDir::new().expect("temp dir");
    let store = MigrationStore::new_in(dir.path().join("store")).expect("create store");

    store
        .save_migration(&NewMigration {
            name: "m1",
            up_sql: "SELECT 1;",
            down_sql: None,
            source_path: "/a",
            target_path: "/b",
            affected_tables: &["users".to_owned()],
            description: None,
        })
        .expect("save");
    store
        .save_migration(&NewMigration {
            name: "m2",
            up_sql: "SELECT 1;",
            down_sql: None,
            source_path: "/a",
            target_path: "/b",
            affected_tables: &["orders".to_owned()],
            description: None,
        })
        .expect("save");

    let conflicts = store.detect_conflicts().expect("detect");
    assert!(conflicts.is_empty());
}

// --- Affected tables collection ---

#[test]
fn collect_affected_tables_from_diff() {
    let dir = TempDir::new().expect("temp dir");
    let left = create_db(
        &dir,
        "left.db",
        "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);
         CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT);
         INSERT INTO items VALUES (1, 'a');
         INSERT INTO users VALUES (1, 'u');",
    );
    let right = create_db(
        &dir,
        "right.db",
        "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);
         CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT);
         INSERT INTO items VALUES (1, 'changed');
         INSERT INTO users VALUES (1, 'u');
         CREATE TABLE orders (id INTEGER PRIMARY KEY);",
    );

    let diff = patchworks::db::differ::diff_databases(&left, &right).expect("diff");
    let tables = collect_affected_tables(&diff.schema, &diff.data_diffs);

    assert!(tables.contains(&"items".to_owned()), "items was modified");
    assert!(tables.contains(&"orders".to_owned()), "orders was added");
    assert!(!tables.contains(&"users".to_owned()), "users was unchanged");
}

// --- Squash tests ---

#[test]
fn squash_migrations_combines_sequential_changes() {
    let dir = TempDir::new().expect("temp dir");
    let base = create_db(
        &dir,
        "base.db",
        "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);
         INSERT INTO items VALUES (1, 'original');",
    );

    // Create two migration states
    let mid = create_db(
        &dir,
        "mid.db",
        "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);
         INSERT INTO items VALUES (1, 'original'), (2, 'new');",
    );
    let final_db = create_db(
        &dir,
        "final.db",
        "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);
         INSERT INTO items VALUES (1, 'original'), (2, 'new'), (3, 'newest');
         CREATE TABLE orders (id INTEGER PRIMARY KEY);",
    );

    let up1 = generate_up_sql(&base, &mid).expect("up1");
    let up2 = generate_up_sql(&mid, &final_db).expect("up2");

    let migrations = vec![
        patchworks::db::types::Migration {
            id: "m1".to_owned(),
            name: "add-row-2".to_owned(),
            up_sql: up1,
            down_sql: None,
            source_path: base.to_string_lossy().into_owned(),
            target_path: mid.to_string_lossy().into_owned(),
            sequence: 1,
            created_at: "2026-01-01T00:00:00Z".to_owned(),
            validated: false,
            affected_tables: vec!["items".to_owned()],
            description: None,
        },
        patchworks::db::types::Migration {
            id: "m2".to_owned(),
            name: "add-row-3-and-orders".to_owned(),
            up_sql: up2,
            down_sql: None,
            source_path: mid.to_string_lossy().into_owned(),
            target_path: final_db.to_string_lossy().into_owned(),
            sequence: 2,
            created_at: "2026-01-02T00:00:00Z".to_owned(),
            validated: false,
            affected_tables: vec!["items".to_owned(), "orders".to_owned()],
            description: None,
        },
    ];

    let result = squash_migrations(&base, &migrations).expect("squash");
    assert_eq!(result.squashed_migration_names.len(), 2);
    assert!(
        result.up_sql.contains("orders"),
        "squashed SQL should include the orders table"
    );
    // The squashed migration should be valid
    let validation = validate_migration(&base, &final_db, &result.up_sql).expect("validate");
    assert!(validation.success);
    assert!(
        validation.matches_target,
        "squashed migration should produce the final state"
    );
}

// --- Apply migration tests ---

#[test]
fn apply_migration_modifies_database() {
    let dir = TempDir::new().expect("temp dir");
    let target = create_db(
        &dir,
        "target.db",
        "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);
         INSERT INTO items VALUES (1, 'a');",
    );

    let sql = "INSERT INTO items VALUES (2, 'b');";
    let result = apply_migration(&target, sql, false).expect("apply");
    assert!(result.success);

    // Verify the change was actually applied
    let rows = read_all_rows(&target, "items");
    assert_eq!(rows.len(), 2);
}

#[test]
fn apply_migration_dry_run_does_not_modify_database() {
    let dir = TempDir::new().expect("temp dir");
    let target = create_db(
        &dir,
        "target.db",
        "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);
         INSERT INTO items VALUES (1, 'a');",
    );

    let sql = "INSERT INTO items VALUES (2, 'b');";
    let result = apply_migration(&target, sql, true).expect("apply dry run");
    assert!(result.success);

    // Verify the database was NOT modified
    let rows = read_all_rows(&target, "items");
    assert_eq!(rows.len(), 1, "dry run should not modify the database");
}

#[test]
fn apply_migration_reports_failure_for_bad_sql() {
    let dir = TempDir::new().expect("temp dir");
    let target = create_db(
        &dir,
        "target.db",
        "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);",
    );

    let result = apply_migration(&target, "TOTALLY INVALID SQL;", false).expect("apply");
    assert!(!result.success);
    assert!(result.error.is_some());
}

// --- End-to-end migration workflow test ---

#[test]
fn end_to_end_migration_generate_validate_apply() {
    let dir = TempDir::new().expect("temp dir");
    let left = create_db(
        &dir,
        "left.db",
        "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT NOT NULL, price REAL);
         INSERT INTO items VALUES (1, 'Widget', 9.99);
         INSERT INTO items VALUES (2, 'Gadget', 19.99);",
    );
    let right = create_db(
        &dir,
        "right.db",
        "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT NOT NULL, price REAL);
         INSERT INTO items VALUES (1, 'Widget', 12.99);
         INSERT INTO items VALUES (2, 'Gadget', 19.99);
         INSERT INTO items VALUES (3, 'Doohickey', 5.99);",
    );

    // Step 1: Generate
    let up_sql = generate_up_sql(&left, &right).expect("generate up");
    let down_sql = generate_down_sql(&left, &right).expect("generate down");
    assert!(down_sql.is_some());

    // Step 2: Validate
    let validation = validate_migration(&left, &right, &up_sql).expect("validate");
    assert!(validation.success);
    assert!(validation.matches_target);

    // Step 3: Apply (dry run first)
    let apply_target = create_db(
        &dir,
        "apply-target.db",
        "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT NOT NULL, price REAL);
         INSERT INTO items VALUES (1, 'Widget', 9.99);
         INSERT INTO items VALUES (2, 'Gadget', 19.99);",
    );

    let dry_result = apply_migration(&apply_target, &up_sql, true).expect("dry run");
    assert!(dry_result.success);

    // Verify dry run didn't change anything
    let rows = read_all_rows(&apply_target, "items");
    assert_eq!(rows.len(), 2);

    // Step 4: Apply for real
    let real_result = apply_migration(&apply_target, &up_sql, false).expect("apply");
    assert!(real_result.success);

    // Verify the migration was applied
    let rows = read_all_rows(&apply_target, "items");
    assert_eq!(rows.len(), 3);

    // Step 5: Validate rollback
    let rollback_validation =
        validate_rollback(&left, &up_sql, &down_sql.unwrap()).expect("validate rollback");
    assert!(rollback_validation.success);
    assert!(rollback_validation.matches_target);
}

// --- CLI output format tests ---

#[test]
fn cli_migrate_generate_dry_run_produces_output() {
    let dir = TempDir::new().expect("temp dir");
    let left = create_db(
        &dir,
        "left.db",
        "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);
         INSERT INTO items VALUES (1, 'a');",
    );
    let right = create_db(
        &dir,
        "right.db",
        "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);
         INSERT INTO items VALUES (1, 'b');",
    );

    let mut output = Vec::new();
    let code = patchworks::cli::run_migrate_generate(
        &mut output,
        &left,
        &right,
        Some("test-migration"),
        true,
        patchworks::cli::OutputFormat::Human,
    )
    .expect("generate");
    let text = String::from_utf8(output).expect("utf8");

    assert_eq!(code, patchworks::cli::EXIT_OK);
    assert!(text.contains("Dry run"));
    assert!(text.contains("test-migration"));
    assert!(text.contains("UP SQL"));
}

#[test]
fn cli_migrate_generate_json_dry_run() {
    let dir = TempDir::new().expect("temp dir");
    let left = create_db(
        &dir,
        "left.db",
        "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);
         INSERT INTO items VALUES (1, 'a');",
    );
    let right = create_db(
        &dir,
        "right.db",
        "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);
         INSERT INTO items VALUES (1, 'b');",
    );

    let mut output = Vec::new();
    let code = patchworks::cli::run_migrate_generate(
        &mut output,
        &left,
        &right,
        Some("json-test"),
        true,
        patchworks::cli::OutputFormat::Json,
    )
    .expect("generate json");
    let text = String::from_utf8(output).expect("utf8");

    assert_eq!(code, patchworks::cli::EXIT_OK);
    let parsed: serde_json::Value = serde_json::from_str(&text).expect("valid json");
    assert_eq!(parsed["dry_run"], true);
    assert!(parsed["up_sql"].is_string());
    assert_eq!(parsed["name"], "json-test");
}

#[test]
fn cli_migrate_list_empty() {
    let dir = TempDir::new().expect("temp dir");
    // We can't easily test the default store without polluting ~/.patchworks,
    // so we test via the store directly and the list function
    let store = MigrationStore::new_in(dir.path().join("store")).expect("store");
    let list = store.list_migrations().expect("list");
    assert!(list.is_empty());

    let summary = store.chain_summary().expect("summary");
    assert_eq!(summary.total_migrations, 0);
}

// --- Schema change migration tests ---

#[test]
fn migration_handles_table_removal() {
    let dir = TempDir::new().expect("temp dir");
    let left = create_db(
        &dir,
        "left.db",
        "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);
         CREATE TABLE old_table (id INTEGER PRIMARY KEY);
         INSERT INTO items VALUES (1, 'a');",
    );
    let right = create_db(
        &dir,
        "right.db",
        "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);
         INSERT INTO items VALUES (1, 'a');",
    );

    let up_sql = generate_up_sql(&left, &right).expect("generate");
    assert!(up_sql.contains("DROP TABLE"));

    let validation = validate_migration(&left, &right, &up_sql).expect("validate");
    assert!(validation.success);
    assert!(validation.matches_target);
}

#[test]
fn migration_handles_column_addition_via_schema_change() {
    let dir = TempDir::new().expect("temp dir");
    let left = create_db(
        &dir,
        "left.db",
        "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);
         INSERT INTO items VALUES (1, 'widget');",
    );
    let right = create_db(
        &dir,
        "right.db",
        "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT, price REAL);
         INSERT INTO items VALUES (1, 'widget', 9.99);",
    );

    let up_sql = generate_up_sql(&left, &right).expect("generate");
    let validation = validate_migration(&left, &right, &up_sql).expect("validate");
    assert!(validation.success);
    assert!(
        validation.matches_target,
        "column addition migration should validate: {:?}",
        validation.differing_tables
    );
}

#[test]
fn squash_empty_migration_list_errors() {
    let dir = TempDir::new().expect("temp dir");
    let base = create_db(
        &dir,
        "base.db",
        "CREATE TABLE items (id INTEGER PRIMARY KEY);",
    );

    let result = squash_migrations(&base, &[]);
    assert!(result.is_err());
}
