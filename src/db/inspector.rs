//! SQLite database inspection and paginated table reads.

use std::cmp::Ordering;
use std::path::Path;

use rusqlite::types::ValueRef;
use rusqlite::{Connection, OpenFlags, Row};

use crate::db::types::{
    ColumnInfo, DatabaseSummary, SortDirection, SqlValue, TableInfo, TablePage, TableQuery,
    TableSort, ViewInfo,
};
use crate::error::{PatchworksError, Result};

const INTERNAL_ROWID_ALIAS: &str = "__patchworks_rowid";

/// Opens a SQLite connection in read-only mode.
pub fn open_read_only(path: &Path) -> Result<Connection> {
    Ok(Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
    )?)
}

/// Reads database schema information, table counts, and view definitions.
pub fn inspect_database(path: &Path) -> Result<DatabaseSummary> {
    let connection = open_read_only(path)?;
    let mut tables = Vec::new();
    let mut views = Vec::new();

    let mut statement = connection.prepare(
        "
        SELECT type, name, sql
        FROM sqlite_master
        WHERE name NOT LIKE 'sqlite_%'
        ORDER BY type, name
        ",
    )?;

    let entries = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<String>>(2)?,
        ))
    })?;

    for entry in entries {
        let (entry_type, name, create_sql) = entry?;
        if entry_type == "table" {
            let columns = load_columns(&connection, &name)?;
            let primary_key = columns
                .iter()
                .filter(|column| column.is_primary_key)
                .map(|column| column.name.clone())
                .collect::<Vec<_>>();
            let row_count = count_rows(&connection, &name)?;

            tables.push(TableInfo {
                name,
                columns,
                row_count,
                primary_key,
                create_sql,
            });
        } else if entry_type == "view" {
            views.push(ViewInfo { name, create_sql });
        }
    }

    Ok(DatabaseSummary {
        path: path.to_string_lossy().into_owned(),
        tables,
        views,
    })
}

/// Reads a page of table rows with optional sorting.
pub fn read_table_page(path: &Path, table_name: &str, query: &TableQuery) -> Result<TablePage> {
    let summary = inspect_database(path)?;
    let table = summary
        .tables
        .iter()
        .find(|table| table.name == table_name)
        .cloned()
        .ok_or_else(|| PatchworksError::MissingTable {
            table: table_name.to_owned(),
            path: path.to_path_buf(),
        })?;

    let connection = open_read_only(path)?;
    let order_by = build_order_by_clause(&table, query.sort.as_ref())?;
    let offset = query.page.saturating_mul(query.page_size);
    let column_count = table.columns.len();

    let sql = format!(
        "SELECT {} FROM {}{} LIMIT ? OFFSET ?",
        select_column_list(&table.columns),
        quote_identifier(table_name),
        order_by
    );
    let mut statement = connection.prepare(&sql)?;
    let rows = statement.query_map(
        rusqlite::params![query.page_size as i64, offset as i64],
        move |row| read_value_row(row, column_count, 0),
    )?;

    let mut values = Vec::new();
    for row in rows {
        values.push(row?);
    }

    Ok(TablePage {
        table_name: table.name,
        columns: table.columns,
        rows: values,
        page: query.page,
        page_size: query.page_size,
        total_rows: table.row_count,
        sort: query.sort.clone(),
    })
}

/// Loads all rows from a table in column order.
pub fn load_all_rows(path: &Path, table: &TableInfo) -> Result<Vec<Vec<SqlValue>>> {
    let connection = open_read_only(path)?;
    let sql = format!(
        "SELECT {} FROM {}{}",
        select_column_list(&table.columns),
        quote_identifier(&table.name),
        default_order_clause(table)
    );
    let mut statement = connection.prepare(&sql)?;
    let rows = statement.query_map([], move |row| read_value_row(row, table.columns.len(), 0))?;

    let mut values = Vec::new();
    for row in rows {
        values.push(row?);
    }

    Ok(values)
}

/// Returns the best-effort identifier columns for diffing a table.
pub fn identity_columns(table: &TableInfo) -> Vec<String> {
    if table.primary_key.is_empty() {
        vec![INTERNAL_ROWID_ALIAS.to_owned()]
    } else {
        table.primary_key.clone()
    }
}

/// Quotes a SQLite identifier.
pub fn quote_identifier(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

/// Converts a SQLite row reference into Patchworks values.
pub fn read_value_row(
    row: &Row<'_>,
    count: usize,
    offset: usize,
) -> rusqlite::Result<Vec<SqlValue>> {
    let mut values = Vec::with_capacity(count);
    for index in offset..(offset + count) {
        values.push(sql_value_from_ref(row.get_ref(index)?));
    }

    Ok(values)
}

/// Converts a raw SQLite value into a Patchworks value.
pub fn sql_value_from_ref(value: ValueRef<'_>) -> SqlValue {
    match value {
        ValueRef::Null => SqlValue::Null,
        ValueRef::Integer(value) => SqlValue::Integer(value),
        ValueRef::Real(value) => SqlValue::Real(value),
        ValueRef::Text(value) => SqlValue::Text(String::from_utf8_lossy(value).into_owned()),
        ValueRef::Blob(value) => SqlValue::Blob(value.to_vec()),
    }
}

/// Compares two value slices using SQLite-like type ordering.
pub fn compare_value_slices(left: &[SqlValue], right: &[SqlValue]) -> Ordering {
    for (left_value, right_value) in left.iter().zip(right.iter()) {
        let ordering = compare_sql_values(left_value, right_value);
        if ordering != Ordering::Equal {
            return ordering;
        }
    }

    left.len().cmp(&right.len())
}

/// Compares two SQLite values using a stable ordering compatible with diff merging.
pub fn compare_sql_values(left: &SqlValue, right: &SqlValue) -> Ordering {
    use SqlValue::{Blob, Integer, Null, Real, Text};

    let rank = |value: &SqlValue| match value {
        Null => 0,
        Integer(_) | Real(_) => 1,
        Text(_) => 2,
        Blob(_) => 3,
    };

    let rank_ordering = rank(left).cmp(&rank(right));
    if rank_ordering != Ordering::Equal {
        return rank_ordering;
    }

    match (left, right) {
        (Null, Null) => Ordering::Equal,
        (Integer(left), Integer(right)) => left.cmp(right),
        (Real(left), Real(right)) => left.partial_cmp(right).unwrap_or(Ordering::Equal),
        (Integer(left), Real(right)) => {
            (*left as f64).partial_cmp(right).unwrap_or(Ordering::Equal)
        }
        (Real(left), Integer(right)) => left
            .partial_cmp(&(*right as f64))
            .unwrap_or(Ordering::Equal),
        (Text(left), Text(right)) => left.cmp(right),
        (Blob(left), Blob(right)) => left.cmp(right),
        _ => Ordering::Equal,
    }
}

fn load_columns(connection: &Connection, table_name: &str) -> Result<Vec<ColumnInfo>> {
    let pragma = format!("PRAGMA table_info({})", quote_identifier(table_name));
    let mut statement = connection.prepare(&pragma)?;
    let columns = statement.query_map([], |row| {
        let declared_type = row
            .get::<_, Option<String>>(2)?
            .unwrap_or_else(|| "BLOB".to_owned());
        let pk_position = row.get::<_, i64>(5)?;
        Ok((
            row.get::<_, i64>(0)?,
            pk_position,
            ColumnInfo {
                name: row.get(1)?,
                col_type: declared_type,
                nullable: row.get::<_, i64>(3)? == 0,
                default_value: row.get(4)?,
                is_primary_key: pk_position > 0,
            },
        ))
    })?;

    let mut values = Vec::new();
    for column in columns {
        values.push(column?);
    }

    let mut ordered_primary = values
        .iter()
        .filter(|(_, pk_position, _)| *pk_position > 0)
        .cloned()
        .collect::<Vec<_>>();
    ordered_primary.sort_by_key(|(_, pk_position, _)| *pk_position);

    let primary_names = ordered_primary
        .into_iter()
        .map(|(_, _, column)| column.name)
        .collect::<Vec<_>>();

    values.sort_by_key(|(cid, _, _)| *cid);
    let mut all_columns = values
        .into_iter()
        .map(|(_, _, column)| column)
        .collect::<Vec<_>>();
    for column in &mut all_columns {
        column.is_primary_key = primary_names.iter().any(|name| name == &column.name);
    }

    Ok(all_columns)
}

fn count_rows(connection: &Connection, table_name: &str) -> Result<u64> {
    let sql = format!("SELECT COUNT(*) FROM {}", quote_identifier(table_name));
    let count = connection.query_row(&sql, [], |row| row.get::<_, i64>(0))?;
    Ok(count as u64)
}

fn build_order_by_clause(table: &TableInfo, sort: Option<&TableSort>) -> Result<String> {
    match sort {
        Some(sort) => {
            if !table
                .columns
                .iter()
                .any(|column| column.name == sort.column)
            {
                return Err(PatchworksError::InvalidState(format!(
                    "column `{}` does not exist on table `{}`",
                    sort.column, table.name
                )));
            }
            let direction = match sort.direction {
                SortDirection::Asc => "ASC",
                SortDirection::Desc => "DESC",
            };
            Ok(format!(
                " ORDER BY {} {}",
                quote_identifier(&sort.column),
                direction
            ))
        }
        None => Ok(default_order_clause(table)),
    }
}

fn default_order_clause(table: &TableInfo) -> String {
    if table.primary_key.is_empty() {
        " ORDER BY rowid ASC".to_owned()
    } else {
        let columns = table
            .primary_key
            .iter()
            .map(|column| quote_identifier(column))
            .collect::<Vec<_>>()
            .join(", ");
        format!(" ORDER BY {}", columns)
    }
}

fn select_column_list(columns: &[ColumnInfo]) -> String {
    columns
        .iter()
        .map(|column| quote_identifier(&column.name))
        .collect::<Vec<_>>()
        .join(", ")
}
