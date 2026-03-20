//! SQL export for Patchworks diffs.

use std::collections::BTreeMap;
use std::path::Path;

use crate::db::inspector::{load_all_rows, quote_identifier};
use crate::db::types::{DatabaseSummary, SchemaDiff, SqlValue, TableDataDiff, TableInfo};
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
    let mut sql = Vec::new();

    sql.push("BEGIN TRANSACTION;".to_owned());

    for table in &schema_diff.removed_tables {
        sql.push(format!(
            "DROP TABLE IF EXISTS {};",
            quote_identifier(&table.name)
        ));
    }

    for table in &schema_diff.added_tables {
        append_create_and_seed(right_path, table, &mut sql)?;
    }

    for table_diff in &schema_diff.modified_tables {
        let right_table = right_tables.get(&table_diff.table_name).ok_or_else(|| {
            PatchworksError::InvalidState(format!(
                "missing right-side table definition for `{}`",
                table_diff.table_name
            ))
        })?;
        let backup_name = format!("__patchworks_old_{}", right_table.name);
        sql.push(format!(
            "ALTER TABLE {} RENAME TO {};",
            quote_identifier(&right_table.name),
            quote_identifier(&backup_name)
        ));
        append_create_and_seed(right_path, right_table, &mut sql)?;
        sql.push(format!("DROP TABLE {};", quote_identifier(&backup_name)));
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

    sql.push("COMMIT;".to_owned());
    Ok(sql.join("\n"))
}

fn append_create_and_seed(path: &Path, table: &TableInfo, sql: &mut Vec<String>) -> Result<()> {
    let create_sql = table.create_sql.clone().ok_or_else(|| {
        PatchworksError::InvalidState(format!("missing CREATE TABLE SQL for `{}`", table.name))
    })?;
    sql.push(create_sql.trim_end_matches(';').to_owned() + ";");
    for row in load_all_rows(path, table)? {
        sql.push(format!(
            "INSERT INTO {} ({}) VALUES ({});",
            quote_identifier(&table.name),
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
    let primary_key = if table.primary_key.is_empty() {
        vec!["rowid".to_owned()]
    } else {
        table.primary_key.clone()
    };

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

#[cfg(test)]
mod tests {
    use super::where_clause;
    use crate::db::types::SqlValue;
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
}
