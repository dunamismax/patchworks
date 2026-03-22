//! Streaming row-level diff support for SQLite tables.

use std::cmp::Ordering;
use std::collections::BTreeSet;
use std::path::Path;

use rusqlite::Rows;

use crate::db::inspector::{
    compare_sql_values, compare_value_slices, open_read_only, quote_identifier, read_value_row,
};
use crate::db::types::{
    DatabaseSummary, DiffStats, RowModification, SqlValue, TableDataDiff, TableInfo,
};
use crate::error::{PatchworksError, Result};

const LARGE_TABLE_THRESHOLD: u64 = 100_000;
const ROWID_ALIAS: &str = "__patchworks_rowid";

/// Progress update emitted while diffing shared tables.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DataDiffProgress {
    /// Shared table currently being diffed.
    pub table_name: String,
    /// Zero-based index of the current shared table.
    pub table_index: usize,
    /// Total number of shared tables that will be diffed.
    pub total_tables: usize,
}

#[derive(Clone, Debug, PartialEq)]
struct StreamRow {
    pk_values: Vec<SqlValue>,
    row_values: Vec<SqlValue>,
}

#[derive(Clone, Debug, PartialEq)]
enum IdentityExpr {
    RowId,
    Column(String),
}

impl IdentityExpr {
    fn select_sql(&self, alias: Option<&str>) -> String {
        let expression = match self {
            Self::RowId => "rowid".to_owned(),
            Self::Column(column) => quote_identifier(column),
        };

        match alias {
            Some(alias) => format!("{expression} AS {}", quote_identifier(alias)),
            None => expression,
        }
    }

    fn order_sql(&self) -> String {
        match self {
            Self::RowId => "rowid".to_owned(),
            Self::Column(column) => quote_identifier(column),
        }
    }
}

/// Computes row-level diffs for all shared tables between two databases.
pub fn diff_all_tables(
    left_path: &Path,
    right_path: &Path,
    left: &DatabaseSummary,
    right: &DatabaseSummary,
) -> Result<Vec<TableDataDiff>> {
    diff_all_tables_with_progress(left_path, right_path, left, right, |_| {})
}

/// Computes row-level diffs for all shared tables and reports table-level progress.
pub fn diff_all_tables_with_progress<F>(
    left_path: &Path,
    right_path: &Path,
    left: &DatabaseSummary,
    right: &DatabaseSummary,
    mut on_progress: F,
) -> Result<Vec<TableDataDiff>>
where
    F: FnMut(DataDiffProgress),
{
    let shared_tables = left
        .tables
        .iter()
        .filter_map(|left_table| {
            right
                .tables
                .iter()
                .find(|table| table.name == left_table.name)
                .map(|right_table| (left_table, right_table))
        })
        .collect::<Vec<_>>();
    let total_tables = shared_tables.len();
    let mut diffs = Vec::with_capacity(total_tables);

    for (table_index, (left_table, right_table)) in shared_tables.into_iter().enumerate() {
        on_progress(DataDiffProgress {
            table_name: left_table.name.clone(),
            table_index,
            total_tables,
        });
        diffs.push(diff_table(left_path, right_path, left_table, right_table)?);
    }

    Ok(diffs)
}

/// Computes a streaming row diff for a single table.
pub fn diff_table(
    left_path: &Path,
    right_path: &Path,
    left_table: &TableInfo,
    right_table: &TableInfo,
) -> Result<TableDataDiff> {
    let common_columns = shared_column_names(left_table, right_table);
    let same_primary_key = !left_table.primary_key.is_empty()
        && !right_table.primary_key.is_empty()
        && left_table.primary_key == right_table.primary_key
        && left_table
            .primary_key
            .iter()
            .all(|column| common_columns.iter().any(|candidate| candidate == column));

    let mut warnings = Vec::new();
    if !same_primary_key {
        warnings.push(
            "No shared primary key was found. Falling back to table-local row identity (rowid when available, otherwise each table's declared primary key), which may be unreliable after deletes, reinserts, or primary-key changes."
                .to_owned(),
        );
    }
    if left_table.row_count > LARGE_TABLE_THRESHOLD || right_table.row_count > LARGE_TABLE_THRESHOLD
    {
        warnings.push(
            "Large table detected. Diff is streamed, but very large tables may still take noticeable time."
                .to_owned(),
        );
    }

    let comparison_columns = if common_columns.is_empty() && same_primary_key {
        left_table.primary_key.clone()
    } else {
        common_columns.clone()
    };
    let (left_identity, right_identity, identity_embedded_in_values) = if same_primary_key {
        let identity = left_table
            .primary_key
            .iter()
            .cloned()
            .map(IdentityExpr::Column)
            .collect::<Vec<_>>();
        (identity.clone(), identity, true)
    } else if table_supports_rowid(left_table) && table_supports_rowid(right_table) {
        (vec![IdentityExpr::RowId], vec![IdentityExpr::RowId], false)
    } else {
        (
            fallback_identity_exprs(left_table)?,
            fallback_identity_exprs(right_table)?,
            false,
        )
    };

    let left_connection = open_read_only(left_path)?;
    let right_connection = open_read_only(right_path)?;

    let left_sql = build_stream_sql(
        left_table,
        &comparison_columns,
        &left_identity,
        identity_embedded_in_values,
    );
    let right_sql = build_stream_sql(
        right_table,
        &comparison_columns,
        &right_identity,
        identity_embedded_in_values,
    );

    let mut left_statement = left_connection.prepare(&left_sql)?;
    let mut right_statement = right_connection.prepare(&right_sql)?;
    let mut left_rows = left_statement.query([])?;
    let mut right_rows = right_statement.query([])?;
    let mut left_current = next_stream_row(
        &mut left_rows,
        &comparison_columns,
        &left_identity,
        identity_embedded_in_values,
    )?;
    let mut right_current = next_stream_row(
        &mut right_rows,
        &comparison_columns,
        &right_identity,
        identity_embedded_in_values,
    )?;

    let mut added_rows = Vec::new();
    let mut removed_rows = Vec::new();
    let mut removed_row_keys = Vec::new();
    let mut modified_rows = Vec::new();
    let mut stats = DiffStats {
        total_rows_left: left_table.row_count,
        total_rows_right: right_table.row_count,
        ..DiffStats::default()
    };

    loop {
        match (left_current.as_ref(), right_current.as_ref()) {
            (Some(left_row), Some(right_row)) => {
                match compare_value_slices(&left_row.pk_values, &right_row.pk_values) {
                    Ordering::Less => {
                        removed_rows.push(left_row.row_values.clone());
                        removed_row_keys.push(left_row.pk_values.clone());
                        stats.removed += 1;
                        left_current = next_stream_row(
                            &mut left_rows,
                            &comparison_columns,
                            &left_identity,
                            identity_embedded_in_values,
                        )?;
                    }
                    Ordering::Greater => {
                        added_rows.push(right_row.row_values.clone());
                        stats.added += 1;
                        right_current = next_stream_row(
                            &mut right_rows,
                            &comparison_columns,
                            &right_identity,
                            identity_embedded_in_values,
                        )?;
                    }
                    Ordering::Equal => {
                        let changes = comparison_columns
                            .iter()
                            .zip(left_row.row_values.iter().zip(right_row.row_values.iter()))
                            .filter_map(|(column, (left_value, right_value))| {
                                if compare_sql_values(left_value, right_value) == Ordering::Equal {
                                    None
                                } else {
                                    Some(crate::db::types::CellChange {
                                        column: column.clone(),
                                        old_value: left_value.clone(),
                                        new_value: right_value.clone(),
                                    })
                                }
                            })
                            .collect::<Vec<_>>();

                        if changes.is_empty() {
                            stats.unchanged += 1;
                        } else {
                            modified_rows.push(RowModification {
                                primary_key: left_row.pk_values.clone(),
                                changes,
                            });
                            stats.modified += 1;
                        }
                        left_current = next_stream_row(
                            &mut left_rows,
                            &comparison_columns,
                            &left_identity,
                            identity_embedded_in_values,
                        )?;
                        right_current = next_stream_row(
                            &mut right_rows,
                            &comparison_columns,
                            &right_identity,
                            identity_embedded_in_values,
                        )?;
                    }
                }
            }
            (Some(left_row), None) => {
                removed_rows.push(left_row.row_values.clone());
                removed_row_keys.push(left_row.pk_values.clone());
                stats.removed += 1;
                left_current = next_stream_row(
                    &mut left_rows,
                    &comparison_columns,
                    &left_identity,
                    identity_embedded_in_values,
                )?;
            }
            (None, Some(right_row)) => {
                added_rows.push(right_row.row_values.clone());
                stats.added += 1;
                right_current = next_stream_row(
                    &mut right_rows,
                    &comparison_columns,
                    &right_identity,
                    identity_embedded_in_values,
                )?;
            }
            (None, None) => break,
        }
    }

    Ok(TableDataDiff {
        table_name: left_table.name.clone(),
        columns: comparison_columns,
        added_rows,
        removed_rows,
        removed_row_keys,
        modified_rows,
        stats,
        warnings,
    })
}

fn shared_column_names(left: &TableInfo, right: &TableInfo) -> Vec<String> {
    let right_names = right
        .columns
        .iter()
        .map(|column| column.name.clone())
        .collect::<BTreeSet<_>>();
    left.columns
        .iter()
        .filter(|column| right_names.contains(&column.name))
        .map(|column| column.name.clone())
        .collect()
}

fn build_stream_sql(
    table: &TableInfo,
    comparison_columns: &[String],
    identity_columns: &[IdentityExpr],
    identity_embedded_in_values: bool,
) -> String {
    let mut select_terms = if identity_embedded_in_values {
        comparison_columns
            .iter()
            .map(|column| quote_identifier(column))
            .collect::<Vec<_>>()
    } else {
        identity_columns
            .iter()
            .enumerate()
            .map(|(index, column)| column.select_sql(Some(&identity_alias(index))))
            .collect::<Vec<_>>()
    };
    select_terms.extend(
        comparison_columns
            .iter()
            .map(|column| quote_identifier(column)),
    );

    format!(
        "SELECT {} FROM {} ORDER BY {}",
        select_terms.join(", "),
        quote_identifier(&table.name),
        identity_columns
            .iter()
            .map(IdentityExpr::order_sql)
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn next_stream_row(
    rows: &mut Rows<'_>,
    comparison_columns: &[String],
    identity_columns: &[IdentityExpr],
    identity_embedded_in_values: bool,
) -> Result<Option<StreamRow>> {
    let Some(row) = rows.next()? else {
        return Ok(None);
    };

    if identity_embedded_in_values {
        let row_values = read_value_row(row, comparison_columns.len(), 0)?;
        let mut pk_values = Vec::with_capacity(identity_columns.len());
        for key in identity_columns {
            let IdentityExpr::Column(key) = key else {
                return Err(PatchworksError::InvalidState(
                    "embedded diff identity cannot use rowid".to_owned(),
                ));
            };
            let index = comparison_columns
                .iter()
                .position(|column| column == key)
                .ok_or_else(|| {
                    PatchworksError::InvalidState(format!(
                        "primary key column `{key}` missing from comparison set"
                    ))
                })?;
            pk_values.push(row_values[index].clone());
        }

        Ok(Some(StreamRow {
            pk_values,
            row_values,
        }))
    } else {
        let pk_values = read_value_row(row, identity_columns.len(), 0)?;
        let row_values = read_value_row(row, comparison_columns.len(), identity_columns.len())?;
        Ok(Some(StreamRow {
            pk_values,
            row_values,
        }))
    }
}

fn fallback_identity_exprs(table: &TableInfo) -> Result<Vec<IdentityExpr>> {
    if !table.primary_key.is_empty() {
        Ok(table
            .primary_key
            .iter()
            .cloned()
            .map(IdentityExpr::Column)
            .collect())
    } else if table_supports_rowid(table) {
        Ok(vec![IdentityExpr::RowId])
    } else {
        Err(PatchworksError::InvalidState(format!(
            "table `{}` has no shared primary key and no usable row identity for diffing",
            table.name
        )))
    }
}

fn table_supports_rowid(table: &TableInfo) -> bool {
    table
        .create_sql
        .as_ref()
        .map(|sql| !sql.to_ascii_uppercase().contains("WITHOUT ROWID"))
        .unwrap_or(true)
}

fn identity_alias(index: usize) -> String {
    format!("{ROWID_ALIAS}_{index}")
}
