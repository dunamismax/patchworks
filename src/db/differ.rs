//! High-level diff orchestration for Patchworks.

use std::path::Path;

use crate::db::inspector::inspect_database;
use crate::db::types::{
    DatabaseSummary, DiffFilter, DiffSummary, SchemaDiff, SemanticChange, TableDataDiff,
};
use crate::diff::{data, export, schema, semantic};
use crate::error::Result;

/// Background progress stage for a database diff.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DiffProgressPhase {
    /// Inspecting the left-side database.
    InspectingLeft,
    /// Inspecting the right-side database.
    InspectingRight,
    /// Comparing schema metadata after both databases are inspected.
    DiffingSchema,
    /// Diffing one shared table's rows.
    DiffingTable {
        /// Shared table currently being diffed.
        table_name: String,
        /// Zero-based index of the current shared table.
        table_index: usize,
        /// Total number of shared tables that will be diffed.
        total_tables: usize,
    },
    /// Generating the SQL export preview.
    GeneratingSqlExport,
}

/// Progress update emitted while building a full database diff.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiffProgress {
    /// Active phase for the in-flight diff.
    pub phase: DiffProgressPhase,
    /// Number of fully completed steps before the current phase.
    pub completed_steps: usize,
    /// Total step count when known.
    pub total_steps: Option<usize>,
}

/// Complete diff payload used by the UI.
#[derive(Clone, Debug)]
pub struct DatabaseDiff {
    /// Left-side database summary.
    pub left: DatabaseSummary,
    /// Right-side database summary.
    pub right: DatabaseSummary,
    /// Schema-level changes.
    pub schema: SchemaDiff,
    /// Row-level changes for shared tables.
    pub data_diffs: Vec<TableDataDiff>,
    /// SQL migration text for the current diff.
    pub sql_export: String,
    /// Aggregate summary statistics.
    pub summary: DiffSummary,
    /// Detected semantic changes (renames, compatible type shifts).
    pub semantic_changes: Vec<SemanticChange>,
}

/// Inspects two databases and computes schema, data, and SQL-export views.
pub fn diff_databases(left_path: &Path, right_path: &Path) -> Result<DatabaseDiff> {
    diff_databases_with_progress(left_path, right_path, |_| {})
}

/// Inspects two databases and computes schema, data, and SQL-export views with progress updates.
pub fn diff_databases_with_progress<F>(
    left_path: &Path,
    right_path: &Path,
    mut on_progress: F,
) -> Result<DatabaseDiff>
where
    F: FnMut(DiffProgress),
{
    on_progress(DiffProgress {
        phase: DiffProgressPhase::InspectingLeft,
        completed_steps: 0,
        total_steps: None,
    });
    let left = inspect_database(left_path)?;

    on_progress(DiffProgress {
        phase: DiffProgressPhase::InspectingRight,
        completed_steps: 1,
        total_steps: None,
    });
    let right = inspect_database(right_path)?;

    let shared_table_count = left
        .tables
        .iter()
        .filter(|left_table| {
            right
                .tables
                .iter()
                .any(|table| table.name == left_table.name)
        })
        .count();
    let total_steps = shared_table_count + 4;

    on_progress(DiffProgress {
        phase: DiffProgressPhase::DiffingSchema,
        completed_steps: 2,
        total_steps: Some(total_steps),
    });
    let schema = schema::diff_schema(&left, &right);

    let data_diffs =
        data::diff_all_tables_with_progress(left_path, right_path, &left, &right, |progress| {
            on_progress(DiffProgress {
                phase: DiffProgressPhase::DiffingTable {
                    table_name: progress.table_name,
                    table_index: progress.table_index,
                    total_tables: progress.total_tables,
                },
                completed_steps: 3 + progress.table_index,
                total_steps: Some(total_steps),
            });
        })?;

    on_progress(DiffProgress {
        phase: DiffProgressPhase::GeneratingSqlExport,
        completed_steps: 3 + shared_table_count,
        total_steps: Some(total_steps),
    });
    let sql_export = export::export_diff_as_sql(right_path, &left, &right, &schema, &data_diffs)?;

    let summary = compute_diff_summary(&schema, &data_diffs);
    let semantic_changes = semantic::detect_semantic_changes(&left, &right, &schema);

    Ok(DatabaseDiff {
        left,
        right,
        schema,
        data_diffs,
        sql_export,
        summary,
        semantic_changes,
    })
}

/// Computes an aggregate summary of a diff result.
pub fn compute_diff_summary(schema: &SchemaDiff, data_diffs: &[TableDataDiff]) -> DiffSummary {
    let tables_with_changes = data_diffs
        .iter()
        .filter(|d| d.stats.added > 0 || d.stats.removed > 0 || d.stats.modified > 0)
        .count();

    let total_cells_changed: u64 = data_diffs
        .iter()
        .flat_map(|d| &d.modified_rows)
        .map(|m| m.changes.len() as u64)
        .sum();

    DiffSummary {
        tables_compared: data_diffs.len(),
        tables_changed: tables_with_changes,
        tables_unchanged: data_diffs.len() - tables_with_changes,
        tables_added: schema.added_tables.len(),
        tables_removed: schema.removed_tables.len(),
        tables_schema_modified: schema.modified_tables.len(),
        total_rows_added: data_diffs.iter().map(|d| d.stats.added).sum(),
        total_rows_removed: data_diffs.iter().map(|d| d.stats.removed).sum(),
        total_rows_modified: data_diffs.iter().map(|d| d.stats.modified).sum(),
        total_rows_unchanged: data_diffs.iter().map(|d| d.stats.unchanged).sum(),
        total_cells_changed,
        indexes_added: schema.added_indexes.len(),
        indexes_removed: schema.removed_indexes.len(),
        indexes_modified: schema.modified_indexes.len(),
        triggers_added: schema.added_triggers.len(),
        triggers_removed: schema.removed_triggers.len(),
        triggers_modified: schema.modified_triggers.len(),
    }
}

/// Applies a filter to diff results, returning only the matching table diffs.
pub fn filter_data_diffs(data_diffs: &[TableDataDiff], filter: &DiffFilter) -> Vec<TableDataDiff> {
    data_diffs
        .iter()
        .filter(|d| filter.accepts_table(&d.table_name))
        .map(|d| {
            let added_rows = if filter.show_added {
                d.added_rows.clone()
            } else {
                Vec::new()
            };
            let removed_rows = if filter.show_removed {
                d.removed_rows.clone()
            } else {
                Vec::new()
            };
            let removed_row_keys = if filter.show_removed {
                d.removed_row_keys.clone()
            } else {
                Vec::new()
            };
            let modified_rows = if filter.show_modified {
                d.modified_rows.clone()
            } else {
                Vec::new()
            };

            let mut stats = d.stats.clone();
            if !filter.show_added {
                stats.added = 0;
            }
            if !filter.show_removed {
                stats.removed = 0;
            }
            if !filter.show_modified {
                stats.modified = 0;
            }

            TableDataDiff {
                table_name: d.table_name.clone(),
                columns: d.columns.clone(),
                added_rows,
                removed_rows,
                removed_row_keys,
                modified_rows,
                stats,
                warnings: d.warnings.clone(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::path::PathBuf;

    use rusqlite::Connection;
    use tempfile::TempDir;

    #[test]
    fn diff_databases_with_progress_reports_expected_stages() -> Result<()> {
        let fixture = FixtureDbs::new()?;
        let mut progress = Vec::new();

        let diff = diff_databases_with_progress(&fixture.left, &fixture.right, |update| {
            progress.push(update);
        })?;

        assert_eq!(diff.data_diffs.len(), 1);
        assert_eq!(
            progress,
            vec![
                DiffProgress {
                    phase: DiffProgressPhase::InspectingLeft,
                    completed_steps: 0,
                    total_steps: None,
                },
                DiffProgress {
                    phase: DiffProgressPhase::InspectingRight,
                    completed_steps: 1,
                    total_steps: None,
                },
                DiffProgress {
                    phase: DiffProgressPhase::DiffingSchema,
                    completed_steps: 2,
                    total_steps: Some(5),
                },
                DiffProgress {
                    phase: DiffProgressPhase::DiffingTable {
                        table_name: "widgets".to_owned(),
                        table_index: 0,
                        total_tables: 1,
                    },
                    completed_steps: 3,
                    total_steps: Some(5),
                },
                DiffProgress {
                    phase: DiffProgressPhase::GeneratingSqlExport,
                    completed_steps: 4,
                    total_steps: Some(5),
                },
            ]
        );

        Ok(())
    }

    struct FixtureDbs {
        _tempdir: TempDir,
        left: PathBuf,
        right: PathBuf,
    }

    impl FixtureDbs {
        fn new() -> Result<Self> {
            let tempdir = tempfile::tempdir()?;
            let left = Self::create_db_at(
                tempdir.path().join("left.sqlite"),
                &[
                    "CREATE TABLE widgets (id INTEGER PRIMARY KEY, name TEXT NOT NULL);",
                    "INSERT INTO widgets (id, name) VALUES (1, 'left-a'), (2, 'left-b');",
                    "CREATE TABLE only_left (id INTEGER PRIMARY KEY);",
                ],
            )?;
            let right = Self::create_db_at(
                tempdir.path().join("right.sqlite"),
                &[
                    "CREATE TABLE widgets (id INTEGER PRIMARY KEY, name TEXT NOT NULL);",
                    "INSERT INTO widgets (id, name) VALUES (1, 'right');",
                    "CREATE TABLE only_right (id INTEGER PRIMARY KEY);",
                ],
            )?;

            Ok(Self {
                _tempdir: tempdir,
                left,
                right,
            })
        }

        fn create_db_at(path: PathBuf, statements: &[&str]) -> Result<PathBuf> {
            let connection = Connection::open(&path)?;
            for statement in statements {
                connection.execute_batch(statement)?;
            }
            Ok(path)
        }
    }
}
