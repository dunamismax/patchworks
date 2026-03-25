//! Three-way merge support for SQLite databases.
//!
//! Compares two derived databases against a common ancestor to detect conflicts
//! and produce a merged result where possible.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::db::differ::diff_databases;
use crate::db::types::{
    CellChange, ConflictKind, MergeConflict, MergeSource, MergeSummary, MergedRowChanges,
    MergedTable, RowModification, SchemaDiff, SqlValue, TableDataDiff, TableSchemaDiff,
    ThreeWayMergeResult,
};
use crate::error::Result;

/// Performs a three-way merge: ancestor vs left, ancestor vs right.
///
/// Returns a merge result with resolved tables and conflicts.
pub fn three_way_merge(
    ancestor_path: &Path,
    left_path: &Path,
    right_path: &Path,
) -> Result<ThreeWayMergeResult> {
    let ancestor_left = diff_databases(ancestor_path, left_path)?;
    let ancestor_right = diff_databases(ancestor_path, right_path)?;

    let mut resolved_tables = Vec::new();
    let mut conflicts = Vec::new();

    // Collect all table names involved
    let all_tables = collect_all_table_names(
        &ancestor_left.schema,
        &ancestor_right.schema,
        &ancestor_left.data_diffs,
        &ancestor_right.data_diffs,
    );

    for table_name in &all_tables {
        merge_table(
            table_name,
            &ancestor_left.schema,
            &ancestor_right.schema,
            &ancestor_left.data_diffs,
            &ancestor_right.data_diffs,
            &mut resolved_tables,
            &mut conflicts,
        );
    }

    let mut row_conflicts = 0;
    let mut schema_conflicts = 0;
    for conflict in &conflicts {
        match &conflict.kind {
            ConflictKind::SchemaConflict { .. } | ConflictKind::TableDeleteConflict { .. } => {
                schema_conflicts += 1;
            }
            ConflictKind::RowConflict { .. } | ConflictKind::DeleteModifyConflict { .. } => {
                row_conflicts += 1;
            }
        }
    }

    let summary = MergeSummary {
        tables_resolved: resolved_tables.len(),
        tables_conflicted: conflicts
            .iter()
            .map(|c| c.table_name.as_str())
            .collect::<BTreeSet<_>>()
            .len(),
        row_conflicts,
        schema_conflicts,
    };

    Ok(ThreeWayMergeResult {
        resolved_tables,
        conflicts,
        summary,
    })
}

fn collect_all_table_names(
    left_schema: &SchemaDiff,
    right_schema: &SchemaDiff,
    left_data: &[TableDataDiff],
    right_data: &[TableDataDiff],
) -> BTreeSet<String> {
    let mut names = BTreeSet::new();

    for table in &left_schema.added_tables {
        names.insert(table.name.clone());
    }
    for table in &left_schema.removed_tables {
        names.insert(table.name.clone());
    }
    for table in &left_schema.modified_tables {
        names.insert(table.table_name.clone());
    }
    for table in &right_schema.added_tables {
        names.insert(table.name.clone());
    }
    for table in &right_schema.removed_tables {
        names.insert(table.name.clone());
    }
    for table in &right_schema.modified_tables {
        names.insert(table.table_name.clone());
    }
    for diff in left_data {
        if diff.stats.added > 0 || diff.stats.removed > 0 || diff.stats.modified > 0 {
            names.insert(diff.table_name.clone());
        }
    }
    for diff in right_data {
        if diff.stats.added > 0 || diff.stats.removed > 0 || diff.stats.modified > 0 {
            names.insert(diff.table_name.clone());
        }
    }

    names
}

#[allow(clippy::too_many_arguments)]
fn merge_table(
    table_name: &str,
    left_schema: &SchemaDiff,
    right_schema: &SchemaDiff,
    left_data: &[TableDataDiff],
    right_data: &[TableDataDiff],
    resolved: &mut Vec<MergedTable>,
    conflicts: &mut Vec<MergeConflict>,
) {
    let left_removed = left_schema
        .removed_tables
        .iter()
        .any(|t| t.name == table_name);
    let right_removed = right_schema
        .removed_tables
        .iter()
        .any(|t| t.name == table_name);
    let left_added = left_schema
        .added_tables
        .iter()
        .any(|t| t.name == table_name);
    let right_added = right_schema
        .added_tables
        .iter()
        .any(|t| t.name == table_name);

    // Both removed: no conflict
    if left_removed && right_removed {
        return;
    }

    // One removed, one modified: conflict
    if left_removed && !right_removed {
        let right_has_changes = has_changes_for_table(table_name, right_schema, right_data);
        if right_has_changes {
            conflicts.push(MergeConflict {
                table_name: table_name.to_owned(),
                kind: ConflictKind::TableDeleteConflict {
                    deleted_by: MergeSource::Left,
                },
            });
        }
        return;
    }
    if right_removed && !left_removed {
        let left_has_changes = has_changes_for_table(table_name, left_schema, left_data);
        if left_has_changes {
            conflicts.push(MergeConflict {
                table_name: table_name.to_owned(),
                kind: ConflictKind::TableDeleteConflict {
                    deleted_by: MergeSource::Right,
                },
            });
        }
        return;
    }

    // Both added the same table: potential conflict or merge
    if left_added && right_added {
        // For simplicity, if both added a table with the same name, that's a schema conflict
        let left_table_schema = left_schema
            .added_tables
            .iter()
            .find(|t| t.name == table_name);
        let right_table_schema = right_schema
            .added_tables
            .iter()
            .find(|t| t.name == table_name);

        if let (Some(_lt), Some(_rt)) = (left_table_schema, right_table_schema) {
            // If schemas match, just take one side
            if left_table_schema == right_table_schema {
                resolved.push(MergedTable {
                    table_name: table_name.to_owned(),
                    source: MergeSource::Both,
                    schema_changes: None,
                    row_changes: MergedRowChanges::default(),
                });
            } else {
                conflicts.push(MergeConflict {
                    table_name: table_name.to_owned(),
                    kind: ConflictKind::SchemaConflict {
                        left_changes: TableSchemaDiff {
                            table_name: table_name.to_owned(),
                            added_columns: Vec::new(),
                            removed_columns: Vec::new(),
                            modified_columns: Vec::new(),
                        },
                        right_changes: TableSchemaDiff {
                            table_name: table_name.to_owned(),
                            added_columns: Vec::new(),
                            removed_columns: Vec::new(),
                            modified_columns: Vec::new(),
                        },
                    },
                });
            }
        }
        return;
    }

    // One side added: take that side
    if left_added {
        resolved.push(MergedTable {
            table_name: table_name.to_owned(),
            source: MergeSource::Left,
            schema_changes: None,
            row_changes: MergedRowChanges::default(),
        });
        return;
    }
    if right_added {
        resolved.push(MergedTable {
            table_name: table_name.to_owned(),
            source: MergeSource::Right,
            schema_changes: None,
            row_changes: MergedRowChanges::default(),
        });
        return;
    }

    // Both sides have the table from the ancestor — check schema changes
    let left_schema_diff = left_schema
        .modified_tables
        .iter()
        .find(|t| t.table_name == table_name);
    let right_schema_diff = right_schema
        .modified_tables
        .iter()
        .find(|t| t.table_name == table_name);

    // Schema conflict: both sides modified the schema differently
    if let (Some(ls), Some(rs)) = (left_schema_diff, right_schema_diff) {
        if ls != rs {
            conflicts.push(MergeConflict {
                table_name: table_name.to_owned(),
                kind: ConflictKind::SchemaConflict {
                    left_changes: ls.clone(),
                    right_changes: rs.clone(),
                },
            });
            return;
        }
    }

    let schema_changes = left_schema_diff.or(right_schema_diff).cloned();

    // Merge row-level changes
    let left_data_diff = left_data.iter().find(|d| d.table_name == table_name);
    let right_data_diff = right_data.iter().find(|d| d.table_name == table_name);

    let (row_changes, row_conflicts) =
        merge_row_changes(table_name, left_data_diff, right_data_diff);

    conflicts.extend(row_conflicts);

    let source = match (
        left_data_diff.map(|d| d.stats.added > 0 || d.stats.removed > 0 || d.stats.modified > 0),
        right_data_diff.map(|d| d.stats.added > 0 || d.stats.removed > 0 || d.stats.modified > 0),
        left_schema_diff.is_some(),
        right_schema_diff.is_some(),
    ) {
        (Some(true), Some(true), _, _) | (_, _, true, true) => MergeSource::Both,
        (Some(true), _, true, _) | (Some(true), _, _, _) | (_, _, true, _) => MergeSource::Left,
        (_, Some(true), _, true) | (_, Some(true), _, _) | (_, _, _, true) => MergeSource::Right,
        _ => MergeSource::Neither,
    };

    if source != MergeSource::Neither || schema_changes.is_some() {
        resolved.push(MergedTable {
            table_name: table_name.to_owned(),
            source,
            schema_changes,
            row_changes,
        });
    }
}

fn has_changes_for_table(table_name: &str, schema: &SchemaDiff, data: &[TableDataDiff]) -> bool {
    schema
        .modified_tables
        .iter()
        .any(|t| t.table_name == table_name)
        || schema.added_tables.iter().any(|t| t.name == table_name)
        || data.iter().any(|d| {
            d.table_name == table_name
                && (d.stats.added > 0 || d.stats.removed > 0 || d.stats.modified > 0)
        })
}

fn merge_row_changes(
    table_name: &str,
    left: Option<&TableDataDiff>,
    right: Option<&TableDataDiff>,
) -> (MergedRowChanges, Vec<MergeConflict>) {
    let mut merged = MergedRowChanges::default();
    let mut conflicts = Vec::new();

    let (left_diff, right_diff) = match (left, right) {
        (None, None) => return (merged, conflicts),
        (Some(l), None) => {
            merged.added_rows = l.added_rows.clone();
            merged.removed_row_keys = l.removed_row_keys.clone();
            merged.modified_rows = l.modified_rows.clone();
            return (merged, conflicts);
        }
        (None, Some(r)) => {
            merged.added_rows = r.added_rows.clone();
            merged.removed_row_keys = r.removed_row_keys.clone();
            merged.modified_rows = r.modified_rows.clone();
            return (merged, conflicts);
        }
        (Some(l), Some(r)) => (l, r),
    };

    // Build maps of changes by primary key for efficient lookup
    let left_removed: BTreeSet<Vec<SqlValue>> =
        left_diff.removed_row_keys.iter().cloned().collect();
    let right_removed: BTreeSet<Vec<SqlValue>> =
        right_diff.removed_row_keys.iter().cloned().collect();
    let left_mods: BTreeMap<Vec<SqlValue>, &RowModification> = left_diff
        .modified_rows
        .iter()
        .map(|m| (m.primary_key.clone(), m))
        .collect();
    let right_mods: BTreeMap<Vec<SqlValue>, &RowModification> = right_diff
        .modified_rows
        .iter()
        .map(|m| (m.primary_key.clone(), m))
        .collect();

    // Removals: union of both, but check for delete-modify conflicts
    for key in &left_removed {
        if let Some(right_mod) = right_mods.get(key) {
            conflicts.push(MergeConflict {
                table_name: table_name.to_owned(),
                kind: ConflictKind::DeleteModifyConflict {
                    primary_key: key.clone(),
                    deleted_by: MergeSource::Left,
                    modifications: right_mod.changes.clone(),
                },
            });
        } else {
            merged.removed_row_keys.push(key.clone());
        }
    }
    for key in &right_removed {
        if left_removed.contains(key) {
            // Both removed: already handled, no conflict
            continue;
        }
        if let Some(left_mod) = left_mods.get(key) {
            conflicts.push(MergeConflict {
                table_name: table_name.to_owned(),
                kind: ConflictKind::DeleteModifyConflict {
                    primary_key: key.clone(),
                    deleted_by: MergeSource::Right,
                    modifications: left_mod.changes.clone(),
                },
            });
        } else {
            merged.removed_row_keys.push(key.clone());
        }
    }

    // Additions: union of both (no conflict if rows are different)
    merged.added_rows.extend(left_diff.added_rows.clone());
    merged.added_rows.extend(right_diff.added_rows.clone());

    // Modifications: check for overlapping changes to the same row
    let all_modified_keys: BTreeSet<&Vec<SqlValue>> =
        left_mods.keys().chain(right_mods.keys()).collect();

    for key in all_modified_keys {
        // Skip if already handled as a delete-modify conflict
        if left_removed.contains(key) || right_removed.contains(key) {
            continue;
        }

        match (left_mods.get(key), right_mods.get(key)) {
            (Some(lm), None) => {
                merged.modified_rows.push((*lm).clone());
            }
            (None, Some(rm)) => {
                merged.modified_rows.push((*rm).clone());
            }
            (Some(lm), Some(rm)) => {
                // Both modified the same row — check if changes are compatible
                match merge_cell_changes(&lm.changes, &rm.changes) {
                    Ok(merged_changes) => {
                        merged.modified_rows.push(RowModification {
                            primary_key: key.clone(),
                            changes: merged_changes,
                        });
                    }
                    Err(()) => {
                        conflicts.push(MergeConflict {
                            table_name: table_name.to_owned(),
                            kind: ConflictKind::RowConflict {
                                primary_key: key.clone(),
                                left_changes: lm.changes.clone(),
                                right_changes: rm.changes.clone(),
                            },
                        });
                    }
                }
            }
            (None, None) => {}
        }
    }

    (merged, conflicts)
}

/// Attempts to merge cell-level changes from two sides.
/// Returns Ok if changes are compatible (different columns, or same column same value).
/// Returns Err if the same column was changed to different values.
fn merge_cell_changes(
    left: &[CellChange],
    right: &[CellChange],
) -> std::result::Result<Vec<CellChange>, ()> {
    let left_by_col: BTreeMap<&str, &CellChange> =
        left.iter().map(|c| (c.column.as_str(), c)).collect();
    let right_by_col: BTreeMap<&str, &CellChange> =
        right.iter().map(|c| (c.column.as_str(), c)).collect();

    let all_cols: BTreeSet<&str> = left_by_col
        .keys()
        .chain(right_by_col.keys())
        .copied()
        .collect();

    let mut merged = Vec::new();

    for col in all_cols {
        match (left_by_col.get(col), right_by_col.get(col)) {
            (Some(lc), None) => merged.push((*lc).clone()),
            (None, Some(rc)) => merged.push((*rc).clone()),
            (Some(lc), Some(rc)) => {
                // Same column changed by both — only OK if they agree on the new value
                if lc.new_value == rc.new_value {
                    merged.push((*lc).clone());
                } else {
                    return Err(());
                }
            }
            (None, None) => {}
        }
    }

    Ok(merged)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use tempfile::TempDir;

    fn create_test_db(dir: &TempDir, name: &str, sql: &str) -> std::path::PathBuf {
        let path = dir.path().join(name);
        let conn = Connection::open(&path).expect("create db");
        conn.execute_batch(sql).expect("execute sql");
        path
    }

    #[test]
    fn three_way_merge_no_conflicts_different_tables() {
        let dir = TempDir::new().unwrap();
        let ancestor = create_test_db(
            &dir,
            "ancestor.db",
            "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);
             INSERT INTO items VALUES (1, 'a'), (2, 'b');",
        );
        let left = create_test_db(
            &dir,
            "left.db",
            "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);
             INSERT INTO items VALUES (1, 'a-left'), (2, 'b');",
        );
        let right = create_test_db(
            &dir,
            "right.db",
            "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);
             INSERT INTO items VALUES (1, 'a'), (2, 'b-right');",
        );

        let result = three_way_merge(&ancestor, &left, &right).unwrap();
        assert!(result.conflicts.is_empty(), "expected no conflicts");
        assert!(result.summary.tables_resolved > 0);
    }

    #[test]
    fn three_way_merge_detects_row_conflict() {
        let dir = TempDir::new().unwrap();
        let ancestor = create_test_db(
            &dir,
            "ancestor.db",
            "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);
             INSERT INTO items VALUES (1, 'original');",
        );
        let left = create_test_db(
            &dir,
            "left.db",
            "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);
             INSERT INTO items VALUES (1, 'left-change');",
        );
        let right = create_test_db(
            &dir,
            "right.db",
            "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);
             INSERT INTO items VALUES (1, 'right-change');",
        );

        let result = three_way_merge(&ancestor, &left, &right).unwrap();
        assert!(
            !result.conflicts.is_empty(),
            "expected row conflict, got {result:?}"
        );
        assert!(result
            .conflicts
            .iter()
            .any(|c| matches!(&c.kind, ConflictKind::RowConflict { .. })));
    }

    #[test]
    fn three_way_merge_same_change_both_sides() {
        let dir = TempDir::new().unwrap();
        let ancestor = create_test_db(
            &dir,
            "ancestor.db",
            "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);
             INSERT INTO items VALUES (1, 'original');",
        );
        let left = create_test_db(
            &dir,
            "left.db",
            "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);
             INSERT INTO items VALUES (1, 'same-change');",
        );
        let right = create_test_db(
            &dir,
            "right.db",
            "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);
             INSERT INTO items VALUES (1, 'same-change');",
        );

        let result = three_way_merge(&ancestor, &left, &right).unwrap();
        assert!(
            result.conflicts.is_empty(),
            "expected no conflicts when both sides make same change"
        );
    }

    #[test]
    fn three_way_merge_delete_modify_conflict() {
        let dir = TempDir::new().unwrap();
        let ancestor = create_test_db(
            &dir,
            "ancestor.db",
            "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);
             INSERT INTO items VALUES (1, 'a'), (2, 'b');",
        );
        let left = create_test_db(
            &dir,
            "left.db",
            "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);
             INSERT INTO items VALUES (2, 'b');",
        );
        let right = create_test_db(
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
            "expected delete-modify conflict, got {result:?}"
        );
    }

    #[test]
    fn three_way_merge_table_added_by_one_side() {
        let dir = TempDir::new().unwrap();
        let ancestor = create_test_db(
            &dir,
            "ancestor.db",
            "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);
             INSERT INTO items VALUES (1, 'a');",
        );
        let left = create_test_db(
            &dir,
            "left.db",
            "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);
             INSERT INTO items VALUES (1, 'a');
             CREATE TABLE extras (id INTEGER PRIMARY KEY);",
        );
        let right = create_test_db(
            &dir,
            "right.db",
            "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);
             INSERT INTO items VALUES (1, 'a');",
        );

        let result = three_way_merge(&ancestor, &left, &right).unwrap();
        assert!(
            result.conflicts.is_empty(),
            "expected no conflict when only one side adds a table"
        );
        assert!(result
            .resolved_tables
            .iter()
            .any(|t| t.table_name == "extras" && t.source == MergeSource::Left));
    }

    #[test]
    fn three_way_merge_schema_conflict() {
        let dir = TempDir::new().unwrap();
        let ancestor = create_test_db(
            &dir,
            "ancestor.db",
            "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);
             INSERT INTO items VALUES (1, 'a');",
        );
        let left = create_test_db(
            &dir,
            "left.db",
            "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT, price REAL);
             INSERT INTO items VALUES (1, 'a', 9.99);",
        );
        let right = create_test_db(
            &dir,
            "right.db",
            "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT, quantity INTEGER);
             INSERT INTO items VALUES (1, 'a', 5);",
        );

        let result = three_way_merge(&ancestor, &left, &right).unwrap();
        assert!(
            result
                .conflicts
                .iter()
                .any(|c| matches!(&c.kind, ConflictKind::SchemaConflict { .. })),
            "expected schema conflict, got {result:?}"
        );
    }

    #[test]
    fn merge_cell_changes_compatible() {
        let left = vec![CellChange {
            column: "name".to_owned(),
            old_value: SqlValue::Text("old".to_owned()),
            new_value: SqlValue::Text("new-left".to_owned()),
        }];
        let right = vec![CellChange {
            column: "email".to_owned(),
            old_value: SqlValue::Text("old@example.com".to_owned()),
            new_value: SqlValue::Text("new@example.com".to_owned()),
        }];

        let result = merge_cell_changes(&left, &right);
        assert!(result.is_ok());
        let merged = result.unwrap();
        assert_eq!(merged.len(), 2);
    }

    #[test]
    fn merge_cell_changes_conflict() {
        let left = vec![CellChange {
            column: "name".to_owned(),
            old_value: SqlValue::Text("old".to_owned()),
            new_value: SqlValue::Text("left-value".to_owned()),
        }];
        let right = vec![CellChange {
            column: "name".to_owned(),
            old_value: SqlValue::Text("old".to_owned()),
            new_value: SqlValue::Text("right-value".to_owned()),
        }];

        let result = merge_cell_changes(&left, &right);
        assert!(result.is_err());
    }
}
