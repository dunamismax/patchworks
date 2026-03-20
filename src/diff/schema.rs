//! Schema diffing for SQLite tables and columns.

use std::collections::BTreeMap;

use crate::db::types::{ColumnInfo, DatabaseSummary, SchemaDiff, TableInfo, TableSchemaDiff};

/// Computes a schema diff between two inspected databases.
pub fn diff_schema(left: &DatabaseSummary, right: &DatabaseSummary) -> SchemaDiff {
    let left_tables = left
        .tables
        .iter()
        .map(|table| (table.name.clone(), table))
        .collect::<BTreeMap<_, _>>();
    let right_tables = right
        .tables
        .iter()
        .map(|table| (table.name.clone(), table))
        .collect::<BTreeMap<_, _>>();

    let mut added_tables = Vec::new();
    let mut removed_tables = Vec::new();
    let mut modified_tables = Vec::new();
    let mut unchanged_tables = Vec::new();

    for (name, right_table) in &right_tables {
        if let Some(left_table) = left_tables.get(name) {
            if let Some(diff) = diff_table_schema(left_table, right_table) {
                modified_tables.push(diff);
            } else {
                unchanged_tables.push(name.clone());
            }
        } else {
            added_tables.push((**right_table).clone());
        }
    }

    for (name, left_table) in &left_tables {
        if !right_tables.contains_key(name) {
            removed_tables.push((**left_table).clone());
        }
    }

    SchemaDiff {
        added_tables,
        removed_tables,
        modified_tables,
        unchanged_tables,
    }
}

fn diff_table_schema(left: &TableInfo, right: &TableInfo) -> Option<TableSchemaDiff> {
    let left_columns = left
        .columns
        .iter()
        .map(|column| (column.name.clone(), column))
        .collect::<BTreeMap<_, _>>();
    let right_columns = right
        .columns
        .iter()
        .map(|column| (column.name.clone(), column))
        .collect::<BTreeMap<_, _>>();

    let mut added_columns = Vec::new();
    let mut removed_columns = Vec::new();
    let mut modified_columns = Vec::new();

    for (name, right_column) in &right_columns {
        match left_columns.get(name) {
            Some(left_column) if columns_match(left_column, right_column) => {}
            Some(left_column) => {
                modified_columns.push(((**left_column).clone(), (**right_column).clone()));
            }
            None => added_columns.push((**right_column).clone()),
        }
    }

    for (name, left_column) in &left_columns {
        if !right_columns.contains_key(name) {
            removed_columns.push((**left_column).clone());
        }
    }

    if added_columns.is_empty() && removed_columns.is_empty() && modified_columns.is_empty() {
        None
    } else {
        Some(TableSchemaDiff {
            table_name: left.name.clone(),
            added_columns,
            removed_columns,
            modified_columns,
        })
    }
}

fn columns_match(left: &ColumnInfo, right: &ColumnInfo) -> bool {
    left.name == right.name
        && left.col_type.eq_ignore_ascii_case(&right.col_type)
        && left.nullable == right.nullable
        && left.default_value == right.default_value
        && left.is_primary_key == right.is_primary_key
}
