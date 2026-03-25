//! Phase 7 integration tests: advanced diff intelligence.

use patchworks::db::differ::{diff_databases, filter_data_diffs};
use patchworks::db::types::{
    AnnotationStatus, ConflictKind, DiffAnnotation, DiffFilter, SemanticChange, SqlValue,
};
use patchworks::diff::merge::three_way_merge;
use patchworks::diff::semantic::{is_compatible_type_shift, values_semantically_equal};
use rusqlite::Connection;
use tempfile::TempDir;

fn create_db(dir: &TempDir, name: &str, sql: &str) -> std::path::PathBuf {
    let path = dir.path().join(name);
    let conn = Connection::open(&path).expect("create db");
    conn.execute_batch(sql).expect("execute sql");
    path
}

// ---- Diff Summary Tests ----

#[test]
fn diff_summary_counts_all_change_types() {
    let dir = TempDir::new().unwrap();
    let left = create_db(
        &dir,
        "left.db",
        "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT, qty INTEGER);
         INSERT INTO items VALUES (1, 'a', 10), (2, 'b', 20), (3, 'c', 30);
         CREATE TABLE logs (id INTEGER PRIMARY KEY, msg TEXT);
         INSERT INTO logs VALUES (1, 'hello');",
    );
    let right = create_db(
        &dir,
        "right.db",
        "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT, qty INTEGER);
         INSERT INTO items VALUES (1, 'a-modified', 10), (2, 'b', 25), (4, 'd', 40);
         CREATE TABLE logs (id INTEGER PRIMARY KEY, msg TEXT);
         INSERT INTO logs VALUES (1, 'hello');
         CREATE TABLE new_table (id INTEGER PRIMARY KEY);",
    );

    let diff = diff_databases(&left, &right).unwrap();
    let summary = &diff.summary;

    // items: 1 modified (name), 1 modified (qty), 1 removed (3), 1 added (4), 1 unchanged (1 partial)
    // Actually: id=1 modified, id=2 modified, id=3 removed, id=4 added
    assert!(summary.tables_compared > 0);
    assert!(summary.total_rows_added > 0);
    assert!(summary.total_rows_removed > 0);
    assert!(summary.total_rows_modified > 0);
    assert!(summary.total_cells_changed > 0);
    assert_eq!(summary.tables_added, 1); // new_table
}

#[test]
fn diff_summary_reports_zero_changes_for_identical_databases() {
    let dir = TempDir::new().unwrap();
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

    let diff = diff_databases(&left, &right).unwrap();
    let summary = &diff.summary;

    assert_eq!(summary.total_rows_added, 0);
    assert_eq!(summary.total_rows_removed, 0);
    assert_eq!(summary.total_rows_modified, 0);
    assert_eq!(summary.total_cells_changed, 0);
    assert_eq!(summary.tables_changed, 0);
}

// ---- Diff Filter Tests ----

#[test]
fn filter_by_table_name() {
    let dir = TempDir::new().unwrap();
    let left = create_db(
        &dir,
        "left.db",
        "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);
         CREATE TABLE logs (id INTEGER PRIMARY KEY, msg TEXT);
         INSERT INTO items VALUES (1, 'a');
         INSERT INTO logs VALUES (1, 'hello');",
    );
    let right = create_db(
        &dir,
        "right.db",
        "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);
         CREATE TABLE logs (id INTEGER PRIMARY KEY, msg TEXT);
         INSERT INTO items VALUES (1, 'b');
         INSERT INTO logs VALUES (1, 'world');",
    );

    let diff = diff_databases(&left, &right).unwrap();

    let filter = DiffFilter {
        tables: vec!["items".to_owned()],
        show_added: true,
        show_removed: true,
        show_modified: true,
        show_unchanged: false,
    };

    let filtered = filter_data_diffs(&diff.data_diffs, &filter);
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].table_name, "items");
}

#[test]
fn filter_by_change_type() {
    let dir = TempDir::new().unwrap();
    let left = create_db(
        &dir,
        "left.db",
        "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);
         INSERT INTO items VALUES (1, 'keep'), (2, 'remove'), (3, 'modify');",
    );
    let right = create_db(
        &dir,
        "right.db",
        "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);
         INSERT INTO items VALUES (1, 'keep'), (3, 'modified'), (4, 'new');",
    );

    let diff = diff_databases(&left, &right).unwrap();

    // Show only modified
    let filter = DiffFilter {
        tables: Vec::new(),
        show_added: false,
        show_removed: false,
        show_modified: true,
        show_unchanged: false,
    };

    let filtered = filter_data_diffs(&diff.data_diffs, &filter);
    let items = &filtered[0];
    assert!(items.added_rows.is_empty());
    assert!(items.removed_rows.is_empty());
    assert!(!items.modified_rows.is_empty());
}

#[test]
fn filter_all_shows_everything() {
    let dir = TempDir::new().unwrap();
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
         INSERT INTO items VALUES (1, 'a-mod'), (3, 'new');",
    );

    let diff = diff_databases(&left, &right).unwrap();
    let filtered = filter_data_diffs(&diff.data_diffs, &DiffFilter::all());

    assert_eq!(filtered.len(), diff.data_diffs.len());
    assert_eq!(
        filtered[0].added_rows.len(),
        diff.data_diffs[0].added_rows.len()
    );
}

// ---- Semantic Diff Tests ----

#[test]
fn semantic_detects_table_rename_in_full_diff() {
    let dir = TempDir::new().unwrap();
    let left = create_db(
        &dir,
        "left.db",
        "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, email TEXT);
         INSERT INTO users VALUES (1, 'Alice', 'alice@example.com');",
    );
    let right = create_db(
        &dir,
        "right.db",
        "CREATE TABLE customers (id INTEGER PRIMARY KEY, name TEXT, email TEXT);
         INSERT INTO customers VALUES (1, 'Alice', 'alice@example.com');",
    );

    let diff = diff_databases(&left, &right).unwrap();
    assert!(
        diff.semantic_changes.iter().any(|c| matches!(c,
            SemanticChange::TableRename { left_name, right_name, .. }
            if left_name == "users" && right_name == "customers"
        )),
        "expected table rename detection, got {:?}",
        diff.semantic_changes
    );
}

#[test]
fn semantic_detects_compatible_type_shift_in_full_diff() {
    let dir = TempDir::new().unwrap();
    let left = create_db(
        &dir,
        "left.db",
        "CREATE TABLE items (id INTEGER PRIMARY KEY, count INT);
         INSERT INTO items VALUES (1, 5);",
    );
    let right = create_db(
        &dir,
        "right.db",
        "CREATE TABLE items (id INTEGER PRIMARY KEY, count BIGINT);
         INSERT INTO items VALUES (1, 5);",
    );

    let diff = diff_databases(&left, &right).unwrap();
    assert!(
        diff.semantic_changes.iter().any(|c| matches!(c,
            SemanticChange::CompatibleTypeShift { table_name, column_name, .. }
            if table_name == "items" && column_name == "count"
        )),
        "expected compatible type shift, got {:?}",
        diff.semantic_changes
    );
}

// ---- Data-Type-Aware Comparison Tests ----

#[test]
fn type_aware_integer_real_equivalence() {
    assert!(values_semantically_equal(
        &SqlValue::Integer(42),
        &SqlValue::Real(42.0),
        "NUMERIC"
    ));

    // Not equal when values differ
    assert!(!values_semantically_equal(
        &SqlValue::Integer(42),
        &SqlValue::Real(42.5),
        "NUMERIC"
    ));
}

#[test]
fn type_aware_text_integer_for_numeric_columns() {
    assert!(values_semantically_equal(
        &SqlValue::Text("100".to_owned()),
        &SqlValue::Integer(100),
        "INTEGER"
    ));

    // Not equal for text columns
    assert!(!values_semantically_equal(
        &SqlValue::Text("100".to_owned()),
        &SqlValue::Integer(100),
        "TEXT"
    ));
}

#[test]
fn compatible_type_shifts_work_correctly() {
    assert!(is_compatible_type_shift("INT", "INTEGER"));
    assert!(is_compatible_type_shift("INT", "BIGINT"));
    assert!(is_compatible_type_shift("TINYINT", "SMALLINT"));
    assert!(is_compatible_type_shift("TEXT", "VARCHAR(255)"));
    assert!(is_compatible_type_shift("FLOAT", "REAL"));
    assert!(is_compatible_type_shift("DOUBLE", "REAL"));
    assert!(is_compatible_type_shift("INTEGER", "REAL")); // widening

    // Incompatible
    assert!(!is_compatible_type_shift("TEXT", "INTEGER"));
    assert!(!is_compatible_type_shift("BLOB", "TEXT"));
    assert!(!is_compatible_type_shift("REAL", "INTEGER")); // narrowing
}

// ---- Three-Way Merge Tests ----

#[test]
fn merge_non_overlapping_row_changes() {
    let dir = TempDir::new().unwrap();
    let ancestor = create_db(
        &dir,
        "ancestor.db",
        "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT, price REAL);
         INSERT INTO items VALUES (1, 'a', 1.0), (2, 'b', 2.0), (3, 'c', 3.0);",
    );
    let left = create_db(
        &dir,
        "left.db",
        "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT, price REAL);
         INSERT INTO items VALUES (1, 'a-left', 1.0), (2, 'b', 2.0), (3, 'c', 3.0);",
    );
    let right = create_db(
        &dir,
        "right.db",
        "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT, price REAL);
         INSERT INTO items VALUES (1, 'a', 1.0), (2, 'b', 2.0), (3, 'c-right', 3.0);",
    );

    let result = three_way_merge(&ancestor, &left, &right).unwrap();
    assert!(
        result.conflicts.is_empty(),
        "expected clean merge, got conflicts: {:?}",
        result.conflicts
    );
    assert_eq!(result.summary.tables_conflicted, 0);
}

#[test]
fn merge_both_add_different_rows() {
    let dir = TempDir::new().unwrap();
    let ancestor = create_db(
        &dir,
        "ancestor.db",
        "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);
         INSERT INTO items VALUES (1, 'a');",
    );
    let left = create_db(
        &dir,
        "left.db",
        "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);
         INSERT INTO items VALUES (1, 'a'), (2, 'from-left');",
    );
    let right = create_db(
        &dir,
        "right.db",
        "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);
         INSERT INTO items VALUES (1, 'a'), (3, 'from-right');",
    );

    let result = three_way_merge(&ancestor, &left, &right).unwrap();
    assert!(
        result.conflicts.is_empty(),
        "expected clean merge when adding different rows"
    );

    // Should have both additions merged
    let items_merged = result
        .resolved_tables
        .iter()
        .find(|t| t.table_name == "items")
        .expect("items in resolved");
    assert_eq!(items_merged.row_changes.added_rows.len(), 2);
}

#[test]
fn merge_conflicting_same_row_different_values() {
    let dir = TempDir::new().unwrap();
    let ancestor = create_db(
        &dir,
        "ancestor.db",
        "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);
         INSERT INTO items VALUES (1, 'original');",
    );
    let left = create_db(
        &dir,
        "left.db",
        "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);
         INSERT INTO items VALUES (1, 'left-version');",
    );
    let right = create_db(
        &dir,
        "right.db",
        "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);
         INSERT INTO items VALUES (1, 'right-version');",
    );

    let result = three_way_merge(&ancestor, &left, &right).unwrap();
    assert!(
        !result.conflicts.is_empty(),
        "expected row conflict when both sides change same row differently"
    );
    assert!(result.summary.row_conflicts > 0);
}

#[test]
fn merge_schema_conflict_different_columns_added() {
    let dir = TempDir::new().unwrap();
    let ancestor = create_db(
        &dir,
        "ancestor.db",
        "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);",
    );
    let left = create_db(
        &dir,
        "left.db",
        "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT, price REAL);",
    );
    let right = create_db(
        &dir,
        "right.db",
        "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT, quantity INTEGER);",
    );

    let result = three_way_merge(&ancestor, &left, &right).unwrap();
    assert!(
        result
            .conflicts
            .iter()
            .any(|c| matches!(&c.kind, ConflictKind::SchemaConflict { .. })),
        "expected schema conflict, got {:?}",
        result.conflicts
    );
}

#[test]
fn merge_delete_vs_modify_conflict() {
    let dir = TempDir::new().unwrap();
    let ancestor = create_db(
        &dir,
        "ancestor.db",
        "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);
         INSERT INTO items VALUES (1, 'a'), (2, 'b');",
    );
    let left = create_db(
        &dir,
        "left.db",
        "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);
         INSERT INTO items VALUES (2, 'b');",
    );
    let right = create_db(
        &dir,
        "right.db",
        "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);
         INSERT INTO items VALUES (1, 'a-modified'), (2, 'b');",
    );

    let result = three_way_merge(&ancestor, &left, &right).unwrap();
    assert!(
        result
            .conflicts
            .iter()
            .any(|c| matches!(&c.kind, ConflictKind::DeleteModifyConflict { .. })),
        "expected delete-modify conflict, got {:?}",
        result.conflicts
    );
}

#[test]
fn merge_table_deleted_by_one_side_while_other_modifies() {
    let dir = TempDir::new().unwrap();
    let ancestor = create_db(
        &dir,
        "ancestor.db",
        "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);
         INSERT INTO items VALUES (1, 'a');
         CREATE TABLE extras (id INTEGER PRIMARY KEY);",
    );
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
         INSERT INTO items VALUES (1, 'a');
         CREATE TABLE extras (id INTEGER PRIMARY KEY);
         INSERT INTO extras VALUES (1);",
    );

    let result = three_way_merge(&ancestor, &left, &right).unwrap();
    assert!(
        result.conflicts.iter().any(|c| c.table_name == "extras"
            && matches!(&c.kind, ConflictKind::TableDeleteConflict { .. })),
        "expected table delete conflict for extras, got {:?}",
        result.conflicts
    );
}

#[test]
fn merge_same_change_both_sides_resolves_cleanly() {
    let dir = TempDir::new().unwrap();
    let ancestor = create_db(
        &dir,
        "ancestor.db",
        "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);
         INSERT INTO items VALUES (1, 'old'), (2, 'keep');",
    );
    let left = create_db(
        &dir,
        "left.db",
        "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);
         INSERT INTO items VALUES (1, 'updated'), (2, 'keep');",
    );
    let right = create_db(
        &dir,
        "right.db",
        "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);
         INSERT INTO items VALUES (1, 'updated'), (2, 'keep');",
    );

    let result = three_way_merge(&ancestor, &left, &right).unwrap();
    assert!(
        result.conflicts.is_empty(),
        "expected clean merge when both sides make identical changes"
    );
}

#[test]
fn merge_compatible_column_changes_to_different_columns() {
    let dir = TempDir::new().unwrap();
    let ancestor = create_db(
        &dir,
        "ancestor.db",
        "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT, price REAL, qty INTEGER);
         INSERT INTO items VALUES (1, 'widget', 9.99, 10);",
    );
    let left = create_db(
        &dir,
        "left.db",
        "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT, price REAL, qty INTEGER);
         INSERT INTO items VALUES (1, 'widget-renamed', 9.99, 10);",
    );
    let right = create_db(
        &dir,
        "right.db",
        "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT, price REAL, qty INTEGER);
         INSERT INTO items VALUES (1, 'widget', 12.99, 10);",
    );

    let result = three_way_merge(&ancestor, &left, &right).unwrap();
    assert!(
        result.conflicts.is_empty(),
        "expected clean merge when changes are to different columns, got {:?}",
        result.conflicts
    );

    let items = result
        .resolved_tables
        .iter()
        .find(|t| t.table_name == "items")
        .expect("items in resolved");
    // Both changes should be merged
    assert_eq!(items.row_changes.modified_rows.len(), 1);
    assert_eq!(items.row_changes.modified_rows[0].changes.len(), 2);
}

// ---- Annotation Tests ----

#[test]
fn annotation_status_display() {
    assert_eq!(format!("{}", AnnotationStatus::Pending), "pending");
    assert_eq!(format!("{}", AnnotationStatus::Approved), "approved");
    assert_eq!(format!("{}", AnnotationStatus::Rejected), "rejected");
    assert_eq!(
        format!("{}", AnnotationStatus::NeedsDiscussion),
        "needs-discussion"
    );
    assert_eq!(format!("{}", AnnotationStatus::Deferred), "deferred");
}

#[test]
fn annotations_serialize_and_deserialize() {
    let annotation = DiffAnnotation {
        table_name: "items".to_owned(),
        row_key: Some(vec![SqlValue::Integer(42)]),
        status: AnnotationStatus::Approved,
        note: "Looks good".to_owned(),
    };

    let json = serde_json::to_string(&annotation).unwrap();
    let deserialized: DiffAnnotation = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.table_name, "items");
    assert_eq!(deserialized.status, AnnotationStatus::Approved);
    assert_eq!(deserialized.note, "Looks good");
}

// ---- Column-Level Change Tests ----

#[test]
fn diff_captures_column_level_changes() {
    let dir = TempDir::new().unwrap();
    let left = create_db(
        &dir,
        "left.db",
        "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT, price REAL, qty INTEGER);
         INSERT INTO items VALUES (1, 'widget', 9.99, 10);
         INSERT INTO items VALUES (2, 'gadget', 19.99, 5);",
    );
    let right = create_db(
        &dir,
        "right.db",
        "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT, price REAL, qty INTEGER);
         INSERT INTO items VALUES (1, 'widget', 12.99, 10);
         INSERT INTO items VALUES (2, 'gadget-pro', 19.99, 3);",
    );

    let diff = diff_databases(&left, &right).unwrap();
    let items = &diff.data_diffs[0];

    assert_eq!(items.modified_rows.len(), 2);

    // Row 1: only price changed
    let row1 = &items.modified_rows[0];
    assert_eq!(row1.changes.len(), 1);
    assert_eq!(row1.changes[0].column, "price");

    // Row 2: name and qty changed
    let row2 = &items.modified_rows[1];
    assert_eq!(row2.changes.len(), 2);
    let changed_cols: Vec<&str> = row2.changes.iter().map(|c| c.column.as_str()).collect();
    assert!(changed_cols.contains(&"name"));
    assert!(changed_cols.contains(&"qty"));
}

// ---- Merge Summary Tests ----

#[test]
fn merge_summary_counts_are_accurate() {
    let dir = TempDir::new().unwrap();
    let ancestor = create_db(
        &dir,
        "ancestor.db",
        "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);
         INSERT INTO items VALUES (1, 'a'), (2, 'b'), (3, 'c');
         CREATE TABLE logs (id INTEGER PRIMARY KEY, msg TEXT);
         INSERT INTO logs VALUES (1, 'log');",
    );
    let left = create_db(
        &dir,
        "left.db",
        "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);
         INSERT INTO items VALUES (1, 'a-left'), (2, 'b'), (3, 'c');
         CREATE TABLE logs (id INTEGER PRIMARY KEY, msg TEXT);
         INSERT INTO logs VALUES (1, 'log-left');",
    );
    let right = create_db(
        &dir,
        "right.db",
        "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);
         INSERT INTO items VALUES (1, 'a-right'), (2, 'b'), (3, 'c');
         CREATE TABLE logs (id INTEGER PRIMARY KEY, msg TEXT);
         INSERT INTO logs VALUES (1, 'log');",
    );

    let result = three_way_merge(&ancestor, &left, &right).unwrap();

    // items: row 1 conflict (both changed differently)
    // logs: only left changed
    assert!(result.summary.row_conflicts > 0); // items row 1
    assert!(result.summary.tables_resolved > 0); // logs
}

// ---- Edge Cases ----

#[test]
fn merge_empty_ancestor_with_different_additions() {
    let dir = TempDir::new().unwrap();
    let ancestor = create_db(&dir, "ancestor.db", "");
    let left = create_db(
        &dir,
        "left.db",
        "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);",
    );
    let right = create_db(
        &dir,
        "right.db",
        "CREATE TABLE logs (id INTEGER PRIMARY KEY, msg TEXT);",
    );

    let result = three_way_merge(&ancestor, &left, &right).unwrap();
    assert!(
        result.conflicts.is_empty(),
        "expected clean merge when adding different tables to empty ancestor"
    );
    assert!(result
        .resolved_tables
        .iter()
        .any(|t| t.table_name == "items"));
    assert!(result
        .resolved_tables
        .iter()
        .any(|t| t.table_name == "logs"));
}

#[test]
fn diff_summary_includes_schema_object_counts() {
    let dir = TempDir::new().unwrap();
    let left = create_db(
        &dir,
        "left.db",
        "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);
         CREATE INDEX idx_name ON items(name);",
    );
    let right = create_db(
        &dir,
        "right.db",
        "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);
         CREATE INDEX idx_name ON items(name, id);
         CREATE TRIGGER items_trigger AFTER INSERT ON items BEGIN SELECT 1; END;",
    );

    let diff = diff_databases(&left, &right).unwrap();
    let summary = &diff.summary;

    assert_eq!(summary.indexes_modified, 1);
    assert_eq!(summary.triggers_added, 1);
}

#[test]
fn semantic_changes_are_serializable() {
    let changes = vec![
        SemanticChange::TableRename {
            left_name: "users".to_owned(),
            right_name: "customers".to_owned(),
            confidence: 85,
        },
        SemanticChange::ColumnRename {
            table_name: "items".to_owned(),
            left_column: "name".to_owned(),
            right_column: "full_name".to_owned(),
            confidence: 75,
        },
        SemanticChange::CompatibleTypeShift {
            table_name: "items".to_owned(),
            column_name: "count".to_owned(),
            left_type: "INT".to_owned(),
            right_type: "INTEGER".to_owned(),
        },
    ];

    let json = serde_json::to_string_pretty(&changes).unwrap();
    let deserialized: Vec<SemanticChange> = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.len(), 3);
}
