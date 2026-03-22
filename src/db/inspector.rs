//! SQLite database inspection and paginated table reads.

use std::cmp::Ordering;
use std::convert::TryFrom;
use std::path::Path;

use rusqlite::types::ValueRef;
use rusqlite::{Connection, OpenFlags, Row};

use crate::db::types::{
    ColumnInfo, DatabaseSummary, SchemaObjectInfo, SortDirection, SqlValue, TableInfo, TablePage,
    TableQuery, TableSort, ViewInfo,
};
use crate::error::{PatchworksError, Result};

const INTERNAL_ROWID_ALIAS: &str = "__patchworks_rowid";

/// Initial inspection payload used when opening a database in the UI.
#[derive(Clone, Debug, PartialEq)]
pub struct InitialInspection {
    /// Full schema summary for the opened database.
    pub summary: DatabaseSummary,
    /// First table selected for browsing, if any.
    pub selected_table: Option<String>,
    /// First page of the selected table, if any.
    pub table_page: Option<TablePage>,
}

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
    let mut indexes = Vec::new();
    let mut triggers = Vec::new();

    let mut statement = connection.prepare(
        "
        SELECT type, name, tbl_name, sql
        FROM sqlite_master
        WHERE name NOT LIKE 'sqlite_%'
        ORDER BY type, name
        ",
    )?;

    let entries = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, Option<String>>(3)?,
        ))
    })?;

    for entry in entries {
        let (entry_type, name, table_name, create_sql) = entry?;
        if entry_type == "table" {
            let normalized_create_sql = normalize_table_create_sql(create_sql, &name);
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
                create_sql: normalized_create_sql,
            });
        } else if entry_type == "view" {
            views.push(ViewInfo { name, create_sql });
        } else if entry_type == "index" {
            indexes.push(SchemaObjectInfo {
                name,
                table_name,
                create_sql,
            });
        } else if entry_type == "trigger" {
            triggers.push(SchemaObjectInfo {
                name,
                table_name,
                create_sql,
            });
        }
    }

    Ok(DatabaseSummary {
        path: path.to_string_lossy().into_owned(),
        tables,
        views,
        indexes,
        triggers,
    })
}

/// Reads the schema summary plus the initial visible table page for a database.
pub fn inspect_database_with_page(path: &Path, query: &TableQuery) -> Result<InitialInspection> {
    let summary = inspect_database(path)?;
    let selected_table = summary.tables.first().map(|table| table.name.clone());
    let table_page = if let Some(table_name) = &selected_table {
        let table = summary
            .tables
            .iter()
            .find(|table| table.name == *table_name)
            .ok_or_else(|| PatchworksError::MissingTable {
                table: table_name.clone(),
                path: path.to_path_buf(),
            })?;
        Some(read_table_page_for_table(path, table, query)?)
    } else {
        None
    };

    Ok(InitialInspection {
        summary,
        selected_table,
        table_page,
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

    read_table_page_for_table(path, &table, query)
}

/// Reads a page of table rows using a preloaded table definition.
pub fn read_table_page_for_table(
    path: &Path,
    table: &TableInfo,
    query: &TableQuery,
) -> Result<TablePage> {
    let table = table.clone();

    let connection = open_read_only(path)?;
    let order_by = build_order_by_clause(&table, query.sort.as_ref())?;
    let offset = query.page.saturating_mul(query.page_size);
    let column_count = table.columns.len();

    let sql = format!(
        "SELECT {} FROM {}{} LIMIT ? OFFSET ?",
        select_column_list(&table.columns),
        quote_identifier(&table.name),
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

fn normalize_table_create_sql(create_sql: Option<String>, table_name: &str) -> Option<String> {
    create_sql.map(|sql| normalize_table_create_sql_text(&sql, table_name))
}

fn normalize_table_create_sql_text(create_sql: &str, table_name: &str) -> String {
    if !is_simple_identifier(table_name) {
        return create_sql.to_owned();
    }

    let Ok(name_start) = create_table_name_start(create_sql) else {
        return create_sql.to_owned();
    };
    let Ok(name_end) = create_table_name_end(create_sql, name_start) else {
        return create_sql.to_owned();
    };

    let suffix = &create_sql[name_end..];
    let normalized_suffix = if suffix.starts_with('(') {
        format!(" {suffix}")
    } else {
        suffix.to_owned()
    };

    format!(
        "{}{}{}",
        &create_sql[..name_start],
        table_name,
        normalized_suffix
    )
}

fn is_simple_identifier(identifier: &str) -> bool {
    let mut chars = identifier.chars();
    match chars.next() {
        Some(first) if first == '_' || first.is_ascii_alphabetic() => {}
        _ => return false,
    }

    chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

fn create_table_name_start(create_sql: &str) -> Result<usize> {
    let mut index = skip_ascii_whitespace(create_sql, 0);
    index = consume_keyword(create_sql, index, "CREATE").ok_or_else(|| {
        PatchworksError::InvalidState(
            "CREATE TABLE SQL did not start with CREATE while normalizing inspection output"
                .to_owned(),
        )
    })?;
    index = skip_ascii_whitespace(create_sql, index);

    if let Some(next) = consume_keyword(create_sql, index, "TEMPORARY") {
        index = skip_ascii_whitespace(create_sql, next);
    } else if let Some(next) = consume_keyword(create_sql, index, "TEMP") {
        index = skip_ascii_whitespace(create_sql, next);
    }

    index = consume_keyword(create_sql, index, "TABLE").ok_or_else(|| {
        PatchworksError::InvalidState(
            "CREATE TABLE SQL did not contain TABLE while normalizing inspection output".to_owned(),
        )
    })?;
    index = skip_ascii_whitespace(create_sql, index);

    if let Some(next) = consume_keyword(create_sql, index, "IF") {
        index = skip_ascii_whitespace(create_sql, next);
        index = consume_keyword(create_sql, index, "NOT").ok_or_else(|| {
            PatchworksError::InvalidState(
                "CREATE TABLE SQL had IF without NOT while normalizing inspection output"
                    .to_owned(),
            )
        })?;
        index = skip_ascii_whitespace(create_sql, index);
        index = consume_keyword(create_sql, index, "EXISTS").ok_or_else(|| {
            PatchworksError::InvalidState(
                "CREATE TABLE SQL had IF NOT without EXISTS while normalizing inspection output"
                    .to_owned(),
            )
        })?;
        index = skip_ascii_whitespace(create_sql, index);
    }

    Ok(index)
}

fn create_table_name_end(create_sql: &str, start: usize) -> Result<usize> {
    let bytes = create_sql.as_bytes();
    let mut index = start;
    let mut quoted_by: Option<u8> = None;

    while let Some(&byte) = bytes.get(index) {
        if let Some(quote) = quoted_by {
            if byte == quote {
                if matches!(quote, b'"' | b'`') && bytes.get(index + 1) == Some(&quote) {
                    index += 2;
                    continue;
                }
                quoted_by = None;
            }
            index += 1;
            continue;
        }

        match byte {
            b'"' => quoted_by = Some(b'"'),
            b'`' => quoted_by = Some(b'`'),
            b'[' => quoted_by = Some(b']'),
            b'(' => break,
            _ if byte.is_ascii_whitespace() => break,
            _ => {}
        }
        index += 1;
    }

    if index == start {
        Err(PatchworksError::InvalidState(
            "CREATE TABLE SQL is missing a table name while normalizing inspection output"
                .to_owned(),
        ))
    } else {
        Ok(index)
    }
}

fn skip_ascii_whitespace(sql: &str, mut index: usize) -> usize {
    while let Some(byte) = sql.as_bytes().get(index) {
        if byte.is_ascii_whitespace() {
            index += 1;
        } else {
            break;
        }
    }
    index
}

fn consume_keyword(sql: &str, index: usize, keyword: &str) -> Option<usize> {
    let end = index.checked_add(keyword.len())?;
    let slice = sql.get(index..end)?;
    if !slice.eq_ignore_ascii_case(keyword) {
        return None;
    }

    match sql[end..].chars().next() {
        Some(ch) if !ch.is_ascii_whitespace() => None,
        _ => Some(end),
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
    u64::try_from(count).map_err(|_| {
        PatchworksError::InvalidState(format!(
            "received a negative row count while inspecting `{table_name}`"
        ))
    })
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
            let mut order_terms = vec![format!("{} {}", quote_identifier(&sort.column), direction)];
            order_terms.extend(stable_tie_breaker_terms(table, Some(sort.column.as_str())));
            Ok(format!(" ORDER BY {}", order_terms.join(", ")))
        }
        None => Ok(default_order_clause(table)),
    }
}

fn default_order_clause(table: &TableInfo) -> String {
    format!(
        " ORDER BY {}",
        stable_tie_breaker_terms(table, None).join(", ")
    )
}

fn stable_tie_breaker_terms(table: &TableInfo, skip_column: Option<&str>) -> Vec<String> {
    if table.primary_key.is_empty() {
        return if skip_column == Some("rowid") {
            Vec::new()
        } else {
            vec!["rowid ASC".to_owned()]
        };
    }

    table
        .primary_key
        .iter()
        .filter(|column| Some(column.as_str()) != skip_column)
        .map(|column| format!("{} ASC", quote_identifier(column)))
        .collect()
}

fn select_column_list(columns: &[ColumnInfo]) -> String {
    columns
        .iter()
        .map(|column| quote_identifier(&column.name))
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::{build_order_by_clause, default_order_clause};
    use crate::db::types::{ColumnInfo, SortDirection, TableInfo, TableSort};

    fn sample_table() -> TableInfo {
        TableInfo {
            name: "items".to_owned(),
            columns: vec![
                ColumnInfo {
                    name: "id".to_owned(),
                    col_type: "INTEGER".to_owned(),
                    nullable: false,
                    default_value: None,
                    is_primary_key: true,
                },
                ColumnInfo {
                    name: "name".to_owned(),
                    col_type: "TEXT".to_owned(),
                    nullable: true,
                    default_value: None,
                    is_primary_key: false,
                },
            ],
            row_count: 0,
            primary_key: vec!["id".to_owned()],
            create_sql: None,
        }
    }

    #[test]
    fn sorted_pages_include_primary_key_tie_breaker() {
        let order = build_order_by_clause(
            &sample_table(),
            Some(&TableSort {
                column: "name".to_owned(),
                direction: SortDirection::Desc,
            }),
        )
        .expect("build order clause");

        assert_eq!(order, " ORDER BY \"name\" DESC, \"id\" ASC");
    }

    #[test]
    fn default_order_clause_uses_primary_key_columns() {
        let order = default_order_clause(&sample_table());

        assert_eq!(order, " ORDER BY \"id\" ASC");
    }
}
