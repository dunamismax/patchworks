//! SQL export for Patchworks diffs.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::db::inspector::{load_all_rows, quote_identifier};
use crate::db::types::{
    DatabaseSummary, SchemaDiff, SchemaObjectInfo, SqlValue, TableDataDiff, TableInfo,
};
use crate::error::{PatchworksError, Result};

/// Generates a SQL migration script that transforms the left database into the right database.
pub fn export_diff_as_sql(
    right_path: &Path,
    left: &DatabaseSummary,
    right: &DatabaseSummary,
    schema_diff: &SchemaDiff,
    data_diffs: &[TableDataDiff],
) -> Result<String> {
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
    let rebuilt_tables = rebuilt_table_names(schema_diff);
    let incrementally_changed_tables = incrementally_changed_table_names(schema_diff, data_diffs);
    let object_changed_tables = schema_object_changed_table_names(schema_diff);
    let trigger_reset_tables = rebuilt_tables
        .union(&incrementally_changed_tables)
        .cloned()
        .chain(object_changed_tables.iter().cloned())
        .collect::<BTreeSet<_>>();
    let index_reset_tables = rebuilt_tables
        .union(&object_changed_tables)
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut sql = Vec::new();

    sql.push("PRAGMA foreign_keys=OFF;".to_owned());
    sql.push("BEGIN TRANSACTION;".to_owned());

    for trigger in &left.triggers {
        if trigger_reset_tables.contains(&trigger.table_name) {
            sql.push(format!(
                "DROP TRIGGER IF EXISTS {};",
                quote_identifier(&trigger.name)
            ));
        }
    }

    for index in &left.indexes {
        if index_reset_tables.contains(&index.table_name) {
            sql.push(format!(
                "DROP INDEX IF EXISTS {};",
                quote_identifier(&index.name)
            ));
        }
    }

    for table in &schema_diff.removed_tables {
        sql.push(format!(
            "DROP TABLE IF EXISTS {};",
            quote_identifier(&table.name)
        ));
    }

    for table in &schema_diff.added_tables {
        append_create_and_seed(right_path, table, &table.name, &mut sql)?;
    }

    for table_diff in &schema_diff.modified_tables {
        let right_table = right_tables.get(&table_diff.table_name).ok_or_else(|| {
            PatchworksError::InvalidState(format!(
                "missing right-side table definition for `{}`",
                table_diff.table_name
            ))
        })?;
        let replacement_name = format!("__patchworks_new_{}", right_table.name);
        append_create_and_seed(right_path, right_table, &replacement_name, &mut sql)?;
        sql.push(format!(
            "DROP TABLE {};",
            quote_identifier(&right_table.name)
        ));
        sql.push(format!(
            "ALTER TABLE {} RENAME TO {};",
            quote_identifier(&replacement_name),
            quote_identifier(&right_table.name)
        ));
    }

    for table_name in &schema_diff.unchanged_tables {
        let table = left_tables.get(table_name).ok_or_else(|| {
            PatchworksError::InvalidState(format!("missing unchanged table `{table_name}`"))
        })?;
        if let Some(data_diff) = data_diffs
            .iter()
            .find(|diff| diff.table_name == *table_name)
        {
            append_incremental_changes(table, data_diff, &mut sql)?;
        }
    }

    for index in &right.indexes {
        if index_reset_tables.contains(&index.table_name) {
            sql.push(schema_object_create_sql(index, "index")?);
        }
    }

    for trigger in &right.triggers {
        if trigger_reset_tables.contains(&trigger.table_name) {
            sql.push(schema_object_create_sql(trigger, "trigger")?);
        }
    }

    sql.push("COMMIT;".to_owned());
    sql.push("PRAGMA foreign_keys=ON;".to_owned());
    Ok(sql.join("\n"))
}

fn rebuilt_table_names(schema_diff: &SchemaDiff) -> BTreeSet<String> {
    schema_diff
        .added_tables
        .iter()
        .map(|table| table.name.clone())
        .chain(
            schema_diff
                .modified_tables
                .iter()
                .map(|table| table.table_name.clone()),
        )
        .collect()
}

fn incrementally_changed_table_names(
    schema_diff: &SchemaDiff,
    data_diffs: &[TableDataDiff],
) -> BTreeSet<String> {
    let unchanged_tables = schema_diff
        .unchanged_tables
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();

    data_diffs
        .iter()
        .filter(|diff| diff.stats.added > 0 || diff.stats.removed > 0 || diff.stats.modified > 0)
        .map(|diff| diff.table_name.clone())
        .filter(|table_name| unchanged_tables.contains(table_name))
        .collect()
}

fn schema_object_changed_table_names(schema_diff: &SchemaDiff) -> BTreeSet<String> {
    schema_diff
        .added_indexes
        .iter()
        .map(|object| object.table_name.clone())
        .chain(
            schema_diff
                .removed_indexes
                .iter()
                .map(|object| object.table_name.clone()),
        )
        .chain(
            schema_diff
                .modified_indexes
                .iter()
                .flat_map(|(left, right)| [left.table_name.clone(), right.table_name.clone()]),
        )
        .chain(
            schema_diff
                .added_triggers
                .iter()
                .map(|object| object.table_name.clone()),
        )
        .chain(
            schema_diff
                .removed_triggers
                .iter()
                .map(|object| object.table_name.clone()),
        )
        .chain(
            schema_diff
                .modified_triggers
                .iter()
                .flat_map(|(left, right)| [left.table_name.clone(), right.table_name.clone()]),
        )
        .collect()
}

fn append_create_and_seed(
    path: &Path,
    table: &TableInfo,
    target_name: &str,
    sql: &mut Vec<String>,
) -> Result<()> {
    sql.push(create_table_sql_for_name(table, target_name)?);
    for row in load_all_rows(path, table)? {
        sql.push(format!(
            "INSERT INTO {} ({}) VALUES ({});",
            quote_identifier(target_name),
            table
                .columns
                .iter()
                .map(|column| quote_identifier(&column.name))
                .collect::<Vec<_>>()
                .join(", "),
            row.iter().map(sql_literal).collect::<Vec<_>>().join(", ")
        ));
    }
    Ok(())
}

fn append_incremental_changes(
    table: &TableInfo,
    data_diff: &TableDataDiff,
    sql: &mut Vec<String>,
) -> Result<()> {
    let primary_key = export_identity_columns(table)?;

    for (index, row) in data_diff.removed_rows.iter().enumerate() {
        let key = if table.primary_key.is_empty() {
            data_diff.removed_row_keys.get(index).unwrap_or(row)
        } else {
            row
        };
        sql.push(format!(
            "DELETE FROM {} WHERE {};",
            quote_identifier(&table.name),
            where_clause(&table.name, &data_diff.columns, key, &primary_key)?
        ));
    }

    for row in &data_diff.added_rows {
        sql.push(format!(
            "INSERT INTO {} ({}) VALUES ({});",
            quote_identifier(&table.name),
            data_diff
                .columns
                .iter()
                .map(|column| quote_identifier(column))
                .collect::<Vec<_>>()
                .join(", "),
            row.iter().map(sql_literal).collect::<Vec<_>>().join(", ")
        ));
    }

    for row in &data_diff.modified_rows {
        let set_clause = row
            .changes
            .iter()
            .map(|change| {
                format!(
                    "{} = {}",
                    quote_identifier(&change.column),
                    sql_literal(&change.new_value)
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        let where_clause = if table.primary_key.is_empty() {
            format!("rowid = {}", sql_literal(&row.primary_key[0]))
        } else {
            primary_key
                .iter()
                .zip(row.primary_key.iter())
                .map(|(column, value)| {
                    format!("{} = {}", quote_identifier(column), sql_literal(value))
                })
                .collect::<Vec<_>>()
                .join(" AND ")
        };
        sql.push(format!(
            "UPDATE {} SET {} WHERE {};",
            quote_identifier(&table.name),
            set_clause,
            where_clause
        ));
    }

    Ok(())
}

fn schema_object_create_sql(object: &SchemaObjectInfo, kind: &str) -> Result<String> {
    object
        .create_sql
        .as_ref()
        .map(|sql| sql.trim_end_matches(';').to_owned() + ";")
        .ok_or_else(|| {
            PatchworksError::InvalidState(format!(
                "missing CREATE {} SQL for `{}`",
                kind, object.name
            ))
        })
}

fn where_clause(
    table_name: &str,
    columns: &[String],
    row: &[SqlValue],
    primary_key: &[String],
) -> Result<String> {
    if primary_key.len() == 1 && primary_key[0] == "rowid" {
        return Ok(format!("rowid = {}", sql_literal(&row[0])));
    }

    let clauses = primary_key
        .iter()
        .map(|key| {
            let index = columns
                .iter()
                .position(|column| column == key)
                .ok_or_else(|| {
                    PatchworksError::InvalidState(format!(
                        "missing primary key column `{key}` while exporting `{table_name}`"
                    ))
                })?;
            let value = row.get(index).ok_or_else(|| {
                PatchworksError::InvalidState(format!(
                    "missing primary key value for column `{key}` while exporting `{table_name}`"
                ))
            })?;
            Ok(format!(
                "{} = {}",
                quote_identifier(key),
                sql_literal(value)
            ))
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(clauses.join(" AND "))
}

fn sql_literal(value: &SqlValue) -> String {
    match value {
        SqlValue::Null => "NULL".to_owned(),
        SqlValue::Integer(value) => value.to_string(),
        SqlValue::Real(value) => {
            if value.is_finite() {
                value.to_string()
            } else {
                "NULL".to_owned()
            }
        }
        SqlValue::Text(value) => format!("'{}'", value.replace('\'', "''")),
        SqlValue::Blob(bytes) => {
            let hex = bytes
                .iter()
                .map(|byte| format!("{byte:02X}"))
                .collect::<String>();
            format!("X'{hex}'")
        }
    }
}

fn export_identity_columns(table: &TableInfo) -> Result<Vec<String>> {
    if table.primary_key.is_empty() {
        if table_supports_rowid(table) {
            Ok(vec!["rowid".to_owned()])
        } else {
            Err(PatchworksError::InvalidState(format!(
                "table `{}` has no primary key and cannot use rowid during SQL export",
                table.name
            )))
        }
    } else {
        Ok(table.primary_key.clone())
    }
}

fn table_supports_rowid(table: &TableInfo) -> bool {
    table
        .create_sql
        .as_ref()
        .map(|sql| !sql.to_ascii_uppercase().contains("WITHOUT ROWID"))
        .unwrap_or(true)
}

fn create_table_sql_for_name(table: &TableInfo, target_name: &str) -> Result<String> {
    let create_sql = table.create_sql.clone().ok_or_else(|| {
        PatchworksError::InvalidState(format!("missing CREATE TABLE SQL for `{}`", table.name))
    })?;
    let trimmed = create_sql.trim_end_matches(';');
    let sql = if table.name == target_name {
        trimmed.to_owned()
    } else {
        rewrite_create_table_name(trimmed, target_name)?
    };
    Ok(sql + ";")
}

fn rewrite_create_table_name(create_sql: &str, target_name: &str) -> Result<String> {
    let name_start = create_table_name_start(create_sql)?;
    let name_end = create_table_name_end(create_sql, name_start)?;

    if name_end <= name_start {
        return Err(PatchworksError::InvalidState(
            "CREATE TABLE SQL has an invalid table-name position while rewriting export".to_owned(),
        ));
    }

    Ok(format!(
        "{}{}{}",
        &create_sql[..name_start],
        target_name,
        &create_sql[name_end..]
    ))
}

fn create_table_name_start(create_sql: &str) -> Result<usize> {
    let mut index = skip_ascii_whitespace(create_sql, 0);
    index = consume_keyword(create_sql, index, "CREATE").ok_or_else(|| {
        PatchworksError::InvalidState(
            "CREATE TABLE SQL did not start with CREATE while rewriting export".to_owned(),
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
            "CREATE TABLE SQL did not contain TABLE while rewriting export".to_owned(),
        )
    })?;
    index = skip_ascii_whitespace(create_sql, index);

    if let Some(next) = consume_keyword(create_sql, index, "IF") {
        index = skip_ascii_whitespace(create_sql, next);
        index = consume_keyword(create_sql, index, "NOT").ok_or_else(|| {
            PatchworksError::InvalidState(
                "CREATE TABLE SQL had IF without NOT while rewriting export".to_owned(),
            )
        })?;
        index = skip_ascii_whitespace(create_sql, index);
        index = consume_keyword(create_sql, index, "EXISTS").ok_or_else(|| {
            PatchworksError::InvalidState(
                "CREATE TABLE SQL had IF NOT without EXISTS while rewriting export".to_owned(),
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
            "CREATE TABLE SQL is missing a table name while rewriting export".to_owned(),
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

#[cfg(test)]
mod tests {
    use super::{create_table_sql_for_name, schema_object_create_sql, where_clause, TableInfo};
    use crate::db::types::{ColumnInfo, SchemaObjectInfo, SqlValue};
    use crate::error::PatchworksError;

    #[test]
    fn where_clause_rejects_missing_primary_key_columns() {
        let error = where_clause(
            "items",
            &[String::from("name")],
            &[SqlValue::Text(String::from("widget"))],
            &[String::from("id")],
        )
        .expect_err("missing primary key column should error");

        assert!(matches!(error, PatchworksError::InvalidState(_)));
        assert!(error
            .to_string()
            .contains("missing primary key column `id` while exporting `items`"));
    }

    #[test]
    fn schema_object_create_sql_requires_source_sql() {
        let error = schema_object_create_sql(
            &SchemaObjectInfo {
                name: String::from("items_name_idx"),
                table_name: String::from("items"),
                create_sql: None,
            },
            "index",
        )
        .expect_err("missing sql should error");

        assert!(matches!(error, PatchworksError::InvalidState(_)));
        assert!(error
            .to_string()
            .contains("missing CREATE index SQL for `items_name_idx`"));
    }

    #[test]
    fn create_table_sql_for_name_rewrites_table_name_for_rebuilds() {
        let sql = create_table_sql_for_name(
            &TableInfo {
                name: "parents".to_owned(),
                columns: vec![ColumnInfo {
                    name: "id".to_owned(),
                    col_type: "INTEGER".to_owned(),
                    nullable: false,
                    default_value: None,
                    is_primary_key: true,
                }],
                row_count: 0,
                primary_key: vec!["id".to_owned()],
                create_sql: Some(
                    "CREATE TABLE IF NOT EXISTS parents (id INTEGER PRIMARY KEY) WITHOUT ROWID"
                        .to_owned(),
                ),
            },
            "__patchworks_new_parents",
        )
        .expect("rewrite create sql");

        assert_eq!(
            sql,
            "CREATE TABLE IF NOT EXISTS __patchworks_new_parents (id INTEGER PRIMARY KEY) WITHOUT ROWID;"
        );
    }
}
