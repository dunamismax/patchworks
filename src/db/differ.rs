//! High-level diff orchestration for Patchworks.

use std::path::Path;

use crate::db::inspector::inspect_database;
use crate::db::types::{DatabaseSummary, SchemaDiff, TableDataDiff};
use crate::diff::{data, export, schema};
use crate::error::Result;

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
}

/// Inspects two databases and computes schema, data, and SQL-export views.
pub fn diff_databases(left_path: &Path, right_path: &Path) -> Result<DatabaseDiff> {
    let left = inspect_database(left_path)?;
    let right = inspect_database(right_path)?;
    let schema = schema::diff_schema(&left, &right);
    let data_diffs = data::diff_all_tables(left_path, right_path, &left, &right)?;
    let sql_export = export::export_diff_as_sql(right_path, &left, &right, &schema, &data_diffs)?;

    Ok(DatabaseDiff {
        left,
        right,
        schema,
        data_diffs,
        sql_export,
    })
}
