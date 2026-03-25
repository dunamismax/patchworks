//! Migration generation, validation, rollback, and squashing.
//!
//! Builds on the existing diff and export engine to produce migration chains
//! that can be stored, validated, replayed, and reversed.

use std::collections::BTreeSet;
use std::path::Path;

use rusqlite::backup::Backup;
use rusqlite::Connection;
use tempfile::NamedTempFile;

use crate::db::differ::diff_databases;
use crate::db::types::{Migration, MigrationValidation, SchemaDiff, TableDataDiff};
use crate::diff::export::export_diff_as_sql;
use crate::error::{PatchworksError, Result};

/// Generates the forward (up) SQL migration from the left database to the right database.
///
/// This wraps the existing export engine: it performs a full diff and then emits SQL.
pub fn generate_up_sql(left_path: &Path, right_path: &Path) -> Result<String> {
    let diff = diff_databases(left_path, right_path)?;
    export_diff_as_sql(
        right_path,
        &diff.left,
        &diff.right,
        &diff.schema,
        &diff.data_diffs,
    )
}

/// Generates the rollback (down) SQL that reverses a forward migration.
///
/// This is the reverse direction: it generates SQL that transforms the right
/// database back into the left database. Returns `None` if rollback generation
/// fails (e.g. due to data loss that cannot be reversed).
pub fn generate_down_sql(left_path: &Path, right_path: &Path) -> Result<Option<String>> {
    // The rollback is just the export from right → left.
    let diff = diff_databases(right_path, left_path)?;
    let sql = export_diff_as_sql(
        left_path,
        &diff.left,
        &diff.right,
        &diff.schema,
        &diff.data_diffs,
    )?;
    // Only return rollback SQL if it actually does something meaningful
    if sql.contains("INSERT INTO")
        || sql.contains("DELETE FROM")
        || sql.contains("UPDATE ")
        || sql.contains("DROP TABLE")
        || sql.contains("CREATE TABLE")
        || sql.contains("ALTER TABLE")
    {
        Ok(Some(sql))
    } else {
        Ok(None)
    }
}

/// Collects the set of table names affected by a diff.
pub fn collect_affected_tables(schema: &SchemaDiff, data_diffs: &[TableDataDiff]) -> Vec<String> {
    let mut tables = BTreeSet::new();

    for table in &schema.added_tables {
        tables.insert(table.name.clone());
    }
    for table in &schema.removed_tables {
        tables.insert(table.name.clone());
    }
    for table_diff in &schema.modified_tables {
        tables.insert(table_diff.table_name.clone());
    }
    for data_diff in data_diffs {
        if data_diff.stats.added > 0 || data_diff.stats.removed > 0 || data_diff.stats.modified > 0
        {
            tables.insert(data_diff.table_name.clone());
        }
    }

    tables.into_iter().collect()
}

/// Validates a migration by applying its SQL to a copy of the source database
/// and comparing the result to the target database.
pub fn validate_migration(
    source_path: &Path,
    target_path: &Path,
    up_sql: &str,
) -> Result<MigrationValidation> {
    // Create a temporary copy of the source database
    let temp_file = NamedTempFile::new()?;
    let temp_path = temp_file.path();
    copy_database(source_path, temp_path)?;

    // Apply the migration SQL to the copy
    let apply_result = apply_sql_to_database(temp_path, up_sql);
    if let Err(error) = apply_result {
        return Ok(MigrationValidation {
            success: false,
            matches_target: false,
            error: Some(format!("Migration failed to apply: {error}")),
            differing_tables: Vec::new(),
        });
    }

    // Diff the result against the target to see if they match
    let diff = diff_databases(temp_path, target_path)?;

    let has_schema_changes = !diff.schema.added_tables.is_empty()
        || !diff.schema.removed_tables.is_empty()
        || !diff.schema.modified_tables.is_empty();

    let has_data_changes = diff
        .data_diffs
        .iter()
        .any(|d| d.stats.added > 0 || d.stats.removed > 0 || d.stats.modified > 0);

    let differing_tables = if has_schema_changes || has_data_changes {
        collect_affected_tables(&diff.schema, &diff.data_diffs)
    } else {
        Vec::new()
    };

    Ok(MigrationValidation {
        success: true,
        matches_target: !has_schema_changes && !has_data_changes,
        error: None,
        differing_tables,
    })
}

/// Validates a rollback migration by applying up_sql then down_sql and checking
/// that the result matches the original source.
pub fn validate_rollback(
    source_path: &Path,
    up_sql: &str,
    down_sql: &str,
) -> Result<MigrationValidation> {
    let temp_file = NamedTempFile::new()?;
    let temp_path = temp_file.path();
    copy_database(source_path, temp_path)?;

    // Apply forward migration
    if let Err(error) = apply_sql_to_database(temp_path, up_sql) {
        return Ok(MigrationValidation {
            success: false,
            matches_target: false,
            error: Some(format!("Forward migration failed to apply: {error}")),
            differing_tables: Vec::new(),
        });
    }

    // Apply rollback
    if let Err(error) = apply_sql_to_database(temp_path, down_sql) {
        return Ok(MigrationValidation {
            success: false,
            matches_target: false,
            error: Some(format!("Rollback migration failed to apply: {error}")),
            differing_tables: Vec::new(),
        });
    }

    // Compare result against original source
    let diff = diff_databases(temp_path, source_path)?;

    let has_schema_changes = !diff.schema.added_tables.is_empty()
        || !diff.schema.removed_tables.is_empty()
        || !diff.schema.modified_tables.is_empty();

    let has_data_changes = diff
        .data_diffs
        .iter()
        .any(|d| d.stats.added > 0 || d.stats.removed > 0 || d.stats.modified > 0);

    let differing_tables = if has_schema_changes || has_data_changes {
        collect_affected_tables(&diff.schema, &diff.data_diffs)
    } else {
        Vec::new()
    };

    Ok(MigrationValidation {
        success: true,
        matches_target: !has_schema_changes && !has_data_changes,
        error: None,
        differing_tables,
    })
}

/// Squashes a sequence of migrations into a single migration SQL.
///
/// This works by applying each migration in order to a copy of a source database,
/// then diffing the final state against the original to produce a single migration.
pub fn squash_migrations(source_path: &Path, migrations: &[Migration]) -> Result<SquashResult> {
    if migrations.is_empty() {
        return Err(PatchworksError::InvalidState(
            "cannot squash an empty migration list".to_owned(),
        ));
    }

    let temp_file = NamedTempFile::new()?;
    let temp_path = temp_file.path();
    copy_database(source_path, temp_path)?;

    // Apply each migration in sequence order
    let mut applied = Vec::new();
    for migration in migrations {
        if let Err(error) = apply_sql_to_database(temp_path, &migration.up_sql) {
            return Err(PatchworksError::InvalidState(format!(
                "squash failed at migration '{}' (seq {}): {error}",
                migration.name, migration.sequence
            )));
        }
        applied.push(migration.name.clone());
    }

    // Diff source → final state to produce the squashed SQL
    let diff = diff_databases(source_path, temp_path)?;
    let squashed_up = export_diff_as_sql(
        temp_path,
        &diff.left,
        &diff.right,
        &diff.schema,
        &diff.data_diffs,
    )?;

    // Attempt to generate rollback for the squashed migration
    let reverse_diff = diff_databases(temp_path, source_path)?;
    let squashed_down = export_diff_as_sql(
        source_path,
        &reverse_diff.left,
        &reverse_diff.right,
        &reverse_diff.schema,
        &reverse_diff.data_diffs,
    )?;

    let down_sql = if squashed_down.contains("INSERT INTO")
        || squashed_down.contains("DELETE FROM")
        || squashed_down.contains("UPDATE ")
        || squashed_down.contains("DROP TABLE")
        || squashed_down.contains("CREATE TABLE")
        || squashed_down.contains("ALTER TABLE")
    {
        Some(squashed_down)
    } else {
        None
    };

    let affected_tables = collect_affected_tables(&diff.schema, &diff.data_diffs);

    Ok(SquashResult {
        up_sql: squashed_up,
        down_sql,
        affected_tables,
        squashed_migration_names: applied,
    })
}

/// Result of squashing multiple migrations into one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SquashResult {
    /// The squashed forward SQL.
    pub up_sql: String,
    /// The squashed rollback SQL, if reversible.
    pub down_sql: Option<String>,
    /// Tables affected by the squashed migration.
    pub affected_tables: Vec<String>,
    /// Names of the migrations that were squashed.
    pub squashed_migration_names: Vec<String>,
}

/// Applies a full migration to a target database (for the `migrate` CLI command).
///
/// When `dry_run` is true, the migration is applied to a temporary copy
/// and the result is validated without modifying the original.
pub fn apply_migration(
    target_path: &Path,
    up_sql: &str,
    dry_run: bool,
) -> Result<MigrationValidation> {
    if dry_run {
        let temp_file = NamedTempFile::new()?;
        let temp_path = temp_file.path();
        copy_database(target_path, temp_path)?;

        match apply_sql_to_database(temp_path, up_sql) {
            Ok(()) => Ok(MigrationValidation {
                success: true,
                matches_target: true,
                error: None,
                differing_tables: Vec::new(),
            }),
            Err(error) => Ok(MigrationValidation {
                success: false,
                matches_target: false,
                error: Some(format!("Dry run failed: {error}")),
                differing_tables: Vec::new(),
            }),
        }
    } else {
        match apply_sql_to_database(target_path, up_sql) {
            Ok(()) => Ok(MigrationValidation {
                success: true,
                matches_target: true,
                error: None,
                differing_tables: Vec::new(),
            }),
            Err(error) => Ok(MigrationValidation {
                success: false,
                matches_target: false,
                error: Some(format!("Migration failed: {error}")),
                differing_tables: Vec::new(),
            }),
        }
    }
}

// --- Internal helpers ---

fn copy_database(source: &Path, target: &Path) -> Result<()> {
    let src_conn = Connection::open(source)?;
    let mut dst_conn = Connection::open(target)?;
    let backup = Backup::new(&src_conn, &mut dst_conn)?;
    backup.step(-1)?;
    Ok(())
}

fn apply_sql_to_database(db_path: &Path, sql: &str) -> Result<()> {
    let connection = Connection::open(db_path)?;
    connection.execute_batch(sql)?;
    Ok(())
}
