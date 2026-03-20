//! Streaming row-level diff support for SQLite tables.

use std::cmp::Ordering;
use std::collections::BTreeSet;
use std::path::Path;

use rusqlite::Rows;

use crate::db::inspector::{
    compare_value_slices, open_read_only, quote_identifier, read_value_row, sql_value_from_ref,
};
use crate::db::types::{
    DatabaseSummary, DiffStats, RowModification, SqlValue, TableDataDiff, TableInfo,
};
use crate::error::{PatchworksError, Result};

const LARGE_TABLE_THRESHOLD: u64 = 100_000;
const ROWID_ALIAS: &str = "__patchworks_rowid";

#[derive(Clone, Debug, PartialEq)]
struct StreamRow {
    pk_values: Vec<SqlValue>,
    row_values: Vec<SqlValue>,
}

/// Computes row-level diffs for all shared tables between two databases.
pub fn diff_all_tables(
    left_path: &Path,
    right_path: &Path,
    left: &DatabaseSummary,
    right: &DatabaseSummary,
) -> Result<Vec<TableDataDiff>> {
    let mut diffs = Vec::new();
    for left_table in &left.tables {
        if let Some(right_table) = right
            .tables
            .iter()
            .find(|table| table.name == left_table.name)
        {
            diffs.push(diff_table(left_path, right_path, left_table, right_table)?);
        }
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
            "No shared primary key was found. Falling back to rowid comparison, which may be unreliable after deletes and reinserts."
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
    let pk_columns = if same_primary_key {
        left_table.primary_key.clone()
    } else {
        vec![ROWID_ALIAS.to_owned()]
    };

    let left_connection = open_read_only(left_path)?;
    let right_connection = open_read_only(right_path)?;

    let left_sql = build_stream_sql(
        left_table,
        &comparison_columns,
        &pk_columns,
        same_primary_key,
    );
    let right_sql = build_stream_sql(
        right_table,
        &comparison_columns,
        &pk_columns,
        same_primary_key,
    );

    let mut left_statement = left_connection.prepare(&left_sql)?;
    let mut right_statement = right_connection.prepare(&right_sql)?;
    let mut left_rows = left_statement.query([])?;
    let mut right_rows = right_statement.query([])?;
    let mut left_current = next_stream_row(
        &mut left_rows,
        &comparison_columns,
        &pk_columns,
        same_primary_key,
    )?;
    let mut right_current = next_stream_row(
        &mut right_rows,
        &comparison_columns,
        &pk_columns,
        same_primary_key,
    )?;

    let mut added_rows = Vec::new();
    let mut removed_rows = Vec::new();
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
                        stats.removed += 1;
                        left_current = next_stream_row(
                            &mut left_rows,
                            &comparison_columns,
                            &pk_columns,
                            same_primary_key,
                        )?;
                    }
                    Ordering::Greater => {
                        added_rows.push(right_row.row_values.clone());
                        stats.added += 1;
                        right_current = next_stream_row(
                            &mut right_rows,
                            &comparison_columns,
                            &pk_columns,
                            same_primary_key,
                        )?;
                    }
                    Ordering::Equal => {
                        let changes = comparison_columns
                            .iter()
                            .zip(left_row.row_values.iter().zip(right_row.row_values.iter()))
                            .filter_map(|(column, (left_value, right_value))| {
                                if left_value == right_value {
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
                            &pk_columns,
                            same_primary_key,
                        )?;
                        right_current = next_stream_row(
                            &mut right_rows,
                            &comparison_columns,
                            &pk_columns,
                            same_primary_key,
                        )?;
                    }
                }
            }
            (Some(left_row), None) => {
                removed_rows.push(left_row.row_values.clone());
                stats.removed += 1;
                left_current = next_stream_row(
                    &mut left_rows,
                    &comparison_columns,
                    &pk_columns,
                    same_primary_key,
                )?;
            }
            (None, Some(right_row)) => {
                added_rows.push(right_row.row_values.clone());
                stats.added += 1;
                right_current = next_stream_row(
                    &mut right_rows,
                    &comparison_columns,
                    &pk_columns,
                    same_primary_key,
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
    pk_columns: &[String],
    same_primary_key: bool,
) -> String {
    if same_primary_key {
        format!(
            "SELECT {} FROM {} ORDER BY {}",
            comparison_columns
                .iter()
                .map(|column| quote_identifier(column))
                .collect::<Vec<_>>()
                .join(", "),
            quote_identifier(&table.name),
            pk_columns
                .iter()
                .map(|column| quote_identifier(column))
                .collect::<Vec<_>>()
                .join(", ")
        )
    } else if comparison_columns.is_empty() {
        format!(
            "SELECT rowid AS {}, rowid AS {} FROM {} ORDER BY rowid",
            quote_identifier(ROWID_ALIAS),
            quote_identifier("__patchworks_value_rowid"),
            quote_identifier(&table.name)
        )
    } else {
        format!(
            "SELECT rowid AS {}, {} FROM {} ORDER BY rowid",
            quote_identifier(ROWID_ALIAS),
            comparison_columns
                .iter()
                .map(|column| quote_identifier(column))
                .collect::<Vec<_>>()
                .join(", "),
            quote_identifier(&table.name)
        )
    }
}

fn next_stream_row(
    rows: &mut Rows<'_>,
    comparison_columns: &[String],
    pk_columns: &[String],
    same_primary_key: bool,
) -> Result<Option<StreamRow>> {
    let Some(row) = rows.next()? else {
        return Ok(None);
    };

    if same_primary_key {
        let row_values = read_value_row(row, comparison_columns.len(), 0)?;
        let mut pk_values = Vec::with_capacity(pk_columns.len());
        for key in pk_columns {
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
        let pk_value = sql_value_from_ref(row.get_ref(0)?);
        let value_count = if comparison_columns.is_empty() {
            1
        } else {
            comparison_columns.len()
        };
        let row_values = read_value_row(
            row,
            value_count,
            1.min(row.as_ref().column_count().saturating_sub(1)),
        )?;
        Ok(Some(StreamRow {
            pk_values: vec![pk_value],
            row_values,
        }))
    }
}
