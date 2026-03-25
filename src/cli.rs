//! Headless CLI commands for Patchworks.
//!
//! Each public function corresponds to a CLI subcommand and writes its output to the provided
//! writer. All functions return an appropriate exit code: 0 for success, 1 for operational errors,
//! and 2 for diff results that found differences (enabling CI gate usage).

use std::io::Write;
use std::path::Path;

use crate::db::differ::diff_databases;
use crate::db::inspector::inspect_database;
use crate::db::migration::MigrationStore;
use crate::db::snapshot::SnapshotStore;
use crate::db::types::{
    ConflictKind, DatabaseSummary, DiffSummary, MergeSource, Migration, MigrationChainSummary,
    SchemaDiff, SemanticChange, TableDataDiff, ThreeWayMergeResult,
};
use crate::diff::export::write_export;
use crate::diff::merge::three_way_merge;
use crate::diff::migration::{
    apply_migration, collect_affected_tables, generate_down_sql, squash_migrations,
    validate_migration,
};
use crate::error::Result;

/// Exit code: operation succeeded with no differences or no actionable result.
pub const EXIT_OK: i32 = 0;

/// Exit code: operational error (bad path, SQLite failure, etc.).
pub const EXIT_ERROR: i32 = 1;

/// Exit code: diff found differences (useful for CI gates).
pub const EXIT_DIFF: i32 = 2;

/// Output format for CLI commands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputFormat {
    /// Human-readable text output.
    Human,
    /// Machine-readable JSON output.
    Json,
}

/// Runs the `inspect` subcommand: prints schema summary for a database.
pub fn run_inspect<W: Write>(writer: &mut W, path: &Path, format: OutputFormat) -> Result<i32> {
    let summary = inspect_database(path)?;

    match format {
        OutputFormat::Human => write_inspect_human(writer, &summary)?,
        OutputFormat::Json => write_inspect_json(writer, &summary)?,
    }

    Ok(EXIT_OK)
}

/// Runs the `diff` subcommand: prints differences between two databases.
pub fn run_diff<W: Write>(
    writer: &mut W,
    left_path: &Path,
    right_path: &Path,
    format: OutputFormat,
) -> Result<i32> {
    let diff = diff_databases(left_path, right_path)?;
    let has_changes = has_any_changes(&diff.schema, &diff.data_diffs);

    match format {
        OutputFormat::Human => write_diff_human(
            writer,
            &diff.schema,
            &diff.data_diffs,
            &diff.summary,
            &diff.semantic_changes,
        )?,
        OutputFormat::Json => write_diff_json(
            writer,
            &diff.schema,
            &diff.data_diffs,
            &diff.summary,
            &diff.semantic_changes,
        )?,
    }

    Ok(if has_changes { EXIT_DIFF } else { EXIT_OK })
}

/// Runs the `merge` subcommand: three-way merge of two databases against a common ancestor.
pub fn run_merge<W: Write>(
    writer: &mut W,
    ancestor_path: &Path,
    left_path: &Path,
    right_path: &Path,
    format: OutputFormat,
) -> Result<i32> {
    let result = three_way_merge(ancestor_path, left_path, right_path)?;
    let has_conflicts = !result.conflicts.is_empty();

    match format {
        OutputFormat::Human => write_merge_human(writer, &result)?,
        OutputFormat::Json => write_merge_json(writer, &result)?,
    }

    Ok(if has_conflicts { EXIT_DIFF } else { EXIT_OK })
}

/// Runs the `export` subcommand: generates SQL migration from left to right.
pub fn run_export<W: Write>(writer: &mut W, left_path: &Path, right_path: &Path) -> Result<i32> {
    let diff = diff_databases(left_path, right_path)?;
    write_export(
        writer,
        right_path,
        &diff.left,
        &diff.right,
        &diff.schema,
        &diff.data_diffs,
    )?;
    Ok(EXIT_OK)
}

/// Runs the `snapshot save` subcommand.
pub fn run_snapshot_save<W: Write>(writer: &mut W, path: &Path, name: Option<&str>) -> Result<i32> {
    let store = SnapshotStore::new_default()?;
    let snapshot_name = name.map(|n| n.to_owned()).unwrap_or_else(|| {
        format!(
            "{} snapshot",
            path.file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("database")
        )
    });
    let snapshot = store.save_snapshot(path, &snapshot_name)?;
    writeln!(writer, "Saved snapshot {} ({})", snapshot.name, snapshot.id)?;
    Ok(EXIT_OK)
}

/// Runs the `snapshot list` subcommand.
pub fn run_snapshot_list<W: Write>(
    writer: &mut W,
    source_path: Option<&Path>,
    format: OutputFormat,
) -> Result<i32> {
    let store = SnapshotStore::new_default()?;
    let snapshots = match source_path {
        Some(path) => store.list_snapshots(path)?,
        None => store.list_all_snapshots()?,
    };

    match format {
        OutputFormat::Human => {
            if snapshots.is_empty() {
                writeln!(writer, "No snapshots found.")?;
            } else {
                let header_id = "ID";
                let header_name = "NAME";
                let header_tables = "TABLES";
                let header_rows = "ROWS";
                let header_created = "CREATED";
                writeln!(
                    writer,
                    "{:<38} {:<30} {:<6} {:<8} {}",
                    header_id, header_name, header_tables, header_rows, header_created
                )?;
                for snapshot in &snapshots {
                    writeln!(
                        writer,
                        "{:<38} {:<30} {:<6} {:<8} {}",
                        snapshot.id,
                        truncate(&snapshot.name, 28),
                        snapshot.table_count,
                        snapshot.total_rows,
                        &snapshot.created_at[..19.min(snapshot.created_at.len())],
                    )?;
                }
            }
        }
        OutputFormat::Json => {
            let json = serde_json::to_string_pretty(&snapshots)?;
            writeln!(writer, "{json}")?;
        }
    }

    Ok(EXIT_OK)
}

/// Runs the `snapshot delete` subcommand.
pub fn run_snapshot_delete<W: Write>(writer: &mut W, snapshot_id: &str) -> Result<i32> {
    let store = SnapshotStore::new_default()?;
    if store.delete_snapshot(snapshot_id)? {
        writeln!(writer, "Deleted snapshot {snapshot_id}")?;
        Ok(EXIT_OK)
    } else {
        writeln!(writer, "Snapshot {snapshot_id} not found.")?;
        Ok(EXIT_ERROR)
    }
}

// --- Migration subcommands ---

/// Runs `migrate generate`: generates a migration from two databases and stores it.
pub fn run_migrate_generate<W: Write>(
    writer: &mut W,
    left_path: &Path,
    right_path: &Path,
    name: Option<&str>,
    dry_run: bool,
    format: OutputFormat,
) -> Result<i32> {
    let diff = diff_databases(left_path, right_path)?;
    let up_sql = crate::diff::export::export_diff_as_sql(
        right_path,
        &diff.left,
        &diff.right,
        &diff.schema,
        &diff.data_diffs,
    )?;
    let down_sql = generate_down_sql(left_path, right_path)?;
    let affected_tables = collect_affected_tables(&diff.schema, &diff.data_diffs);

    let migration_name = name
        .map(|n| n.to_owned())
        .unwrap_or_else(|| format!("migration-{}", chrono::Utc::now().format("%Y%m%d-%H%M%S")));

    if dry_run {
        match format {
            OutputFormat::Human => {
                writeln!(writer, "--- Dry run: migration not saved ---")?;
                writeln!(writer, "Name: {migration_name}")?;
                writeln!(
                    writer,
                    "Affected tables: {}",
                    if affected_tables.is_empty() {
                        "(none)".to_owned()
                    } else {
                        affected_tables.join(", ")
                    }
                )?;
                writeln!(
                    writer,
                    "Reversible: {}",
                    if down_sql.is_some() { "yes" } else { "no" }
                )?;
                writeln!(writer)?;
                writeln!(writer, "--- UP SQL ---")?;
                write!(writer, "{up_sql}")?;
                if let Some(ref down) = down_sql {
                    writeln!(writer)?;
                    writeln!(writer, "--- DOWN SQL ---")?;
                    write!(writer, "{down}")?;
                }
                writeln!(writer)?;
            }
            OutputFormat::Json => {
                #[derive(serde::Serialize)]
                struct DryRunOutput {
                    name: String,
                    up_sql: String,
                    down_sql: Option<String>,
                    affected_tables: Vec<String>,
                    dry_run: bool,
                }
                let output = DryRunOutput {
                    name: migration_name,
                    up_sql,
                    down_sql,
                    affected_tables,
                    dry_run: true,
                };
                let json = serde_json::to_string_pretty(&output)?;
                writeln!(writer, "{json}")?;
            }
        }
        return Ok(EXIT_OK);
    }

    let store = MigrationStore::new_default()?;
    let source_str = left_path.to_string_lossy().into_owned();
    let target_str = right_path.to_string_lossy().into_owned();

    let migration = store.save_migration(&crate::db::migration::NewMigration {
        name: &migration_name,
        up_sql: &up_sql,
        down_sql: down_sql.as_deref(),
        source_path: &source_str,
        target_path: &target_str,
        affected_tables: &affected_tables,
        description: None,
    })?;

    match format {
        OutputFormat::Human => {
            writeln!(
                writer,
                "Generated migration '{}' ({})",
                migration.name, migration.id
            )?;
            writeln!(writer, "Sequence: {}", migration.sequence)?;
            writeln!(
                writer,
                "Affected tables: {}",
                if migration.affected_tables.is_empty() {
                    "(none)".to_owned()
                } else {
                    migration.affected_tables.join(", ")
                }
            )?;
            writeln!(
                writer,
                "Reversible: {}",
                if migration.down_sql.is_some() {
                    "yes"
                } else {
                    "no"
                }
            )?;
        }
        OutputFormat::Json => {
            let json = serde_json::to_string_pretty(&migration)?;
            writeln!(writer, "{json}")?;
        }
    }

    Ok(EXIT_OK)
}

/// Runs `migrate validate`: validates a migration by applying it to a copy.
pub fn run_migrate_validate<W: Write>(
    writer: &mut W,
    migration_id: &str,
    format: OutputFormat,
) -> Result<i32> {
    let store = MigrationStore::new_default()?;
    let migration = store.get_migration(migration_id)?;

    let source_path = std::path::PathBuf::from(&migration.source_path);
    let target_path = std::path::PathBuf::from(&migration.target_path);

    if !source_path.exists() {
        return match format {
            OutputFormat::Human => {
                writeln!(
                    writer,
                    "Source database not found: {}",
                    source_path.display()
                )?;
                Ok(EXIT_ERROR)
            }
            OutputFormat::Json => {
                let json = serde_json::to_string_pretty(&serde_json::json!({
                    "success": false,
                    "error": format!("Source database not found: {}", source_path.display()),
                }))?;
                writeln!(writer, "{json}")?;
                Ok(EXIT_ERROR)
            }
        };
    }

    let validation = validate_migration(&source_path, &target_path, &migration.up_sql)?;

    if validation.success && validation.matches_target {
        store.mark_validated(migration_id)?;
    }

    match format {
        OutputFormat::Human => {
            if validation.success && validation.matches_target {
                writeln!(
                    writer,
                    "✓ Migration '{}' validated successfully — result matches target",
                    migration.name
                )?;
            } else if validation.success {
                writeln!(
                    writer,
                    "⚠ Migration '{}' applied cleanly but result differs from target",
                    migration.name
                )?;
                if !validation.differing_tables.is_empty() {
                    writeln!(
                        writer,
                        "  Differing tables: {}",
                        validation.differing_tables.join(", ")
                    )?;
                }
            } else {
                writeln!(writer, "✗ Migration '{}' failed to apply", migration.name)?;
                if let Some(ref error) = validation.error {
                    writeln!(writer, "  Error: {error}")?;
                }
            }
        }
        OutputFormat::Json => {
            let json = serde_json::to_string_pretty(&validation)?;
            writeln!(writer, "{json}")?;
        }
    }

    Ok(if validation.success && validation.matches_target {
        EXIT_OK
    } else {
        EXIT_ERROR
    })
}

/// Runs `migrate list`: lists all stored migrations.
pub fn run_migrate_list<W: Write>(writer: &mut W, format: OutputFormat) -> Result<i32> {
    let store = MigrationStore::new_default()?;
    let migrations = store.list_migrations()?;
    let summary = store.chain_summary()?;

    match format {
        OutputFormat::Human => write_migrate_list_human(writer, &migrations, &summary)?,
        OutputFormat::Json => {
            #[derive(serde::Serialize)]
            struct ListOutput<'a> {
                migrations: &'a [Migration],
                summary: &'a MigrationChainSummary,
            }
            let output = ListOutput {
                migrations: &migrations,
                summary: &summary,
            };
            let json = serde_json::to_string_pretty(&output)?;
            writeln!(writer, "{json}")?;
        }
    }

    Ok(EXIT_OK)
}

/// Runs `migrate show`: shows details of a specific migration.
pub fn run_migrate_show<W: Write>(
    writer: &mut W,
    migration_id: &str,
    format: OutputFormat,
) -> Result<i32> {
    let store = MigrationStore::new_default()?;
    let migration = store.get_migration(migration_id)?;

    match format {
        OutputFormat::Human => {
            writeln!(writer, "Migration: {} ({})", migration.name, migration.id)?;
            writeln!(writer, "Sequence: {}", migration.sequence)?;
            writeln!(writer, "Created: {}", migration.created_at)?;
            writeln!(
                writer,
                "Validated: {}",
                if migration.validated { "yes" } else { "no" }
            )?;
            writeln!(
                writer,
                "Reversible: {}",
                if migration.down_sql.is_some() {
                    "yes"
                } else {
                    "no"
                }
            )?;
            writeln!(
                writer,
                "Affected tables: {}",
                if migration.affected_tables.is_empty() {
                    "(none)".to_owned()
                } else {
                    migration.affected_tables.join(", ")
                }
            )?;
            if let Some(ref desc) = migration.description {
                writeln!(writer, "Description: {desc}")?;
            }
            writeln!(writer)?;
            writeln!(writer, "--- UP SQL ---")?;
            write!(writer, "{}", migration.up_sql)?;
            if let Some(ref down) = migration.down_sql {
                writeln!(writer)?;
                writeln!(writer, "--- DOWN SQL ---")?;
                write!(writer, "{down}")?;
            }
            writeln!(writer)?;
        }
        OutputFormat::Json => {
            let json = serde_json::to_string_pretty(&migration)?;
            writeln!(writer, "{json}")?;
        }
    }

    Ok(EXIT_OK)
}

/// Runs `migrate delete`: removes a stored migration.
pub fn run_migrate_delete<W: Write>(writer: &mut W, migration_id: &str) -> Result<i32> {
    let store = MigrationStore::new_default()?;
    if store.delete_migration(migration_id)? {
        writeln!(writer, "Deleted migration {migration_id}")?;
        Ok(EXIT_OK)
    } else {
        writeln!(writer, "Migration {migration_id} not found.")?;
        Ok(EXIT_ERROR)
    }
}

/// Runs `migrate apply`: applies a stored migration to a database.
pub fn run_migrate_apply<W: Write>(
    writer: &mut W,
    migration_id: &str,
    target_path: &Path,
    dry_run: bool,
    format: OutputFormat,
) -> Result<i32> {
    let store = MigrationStore::new_default()?;
    let migration = store.get_migration(migration_id)?;

    let result = apply_migration(target_path, &migration.up_sql, dry_run)?;

    match format {
        OutputFormat::Human => {
            if dry_run {
                if result.success {
                    writeln!(
                        writer,
                        "✓ Dry run: migration '{}' would apply cleanly",
                        migration.name
                    )?;
                } else {
                    writeln!(
                        writer,
                        "✗ Dry run: migration '{}' would fail",
                        migration.name
                    )?;
                    if let Some(ref error) = result.error {
                        writeln!(writer, "  Error: {error}")?;
                    }
                }
            } else if result.success {
                writeln!(
                    writer,
                    "✓ Applied migration '{}' to {}",
                    migration.name,
                    target_path.display()
                )?;
            } else {
                writeln!(writer, "✗ Failed to apply migration '{}'", migration.name)?;
                if let Some(ref error) = result.error {
                    writeln!(writer, "  Error: {error}")?;
                }
            }
        }
        OutputFormat::Json => {
            let json = serde_json::to_string_pretty(&result)?;
            writeln!(writer, "{json}")?;
        }
    }

    Ok(if result.success { EXIT_OK } else { EXIT_ERROR })
}

/// Runs `migrate squash`: combines sequential migrations into one.
pub fn run_migrate_squash<W: Write>(
    writer: &mut W,
    source_path: &Path,
    name: Option<&str>,
    dry_run: bool,
    format: OutputFormat,
) -> Result<i32> {
    let store = MigrationStore::new_default()?;
    let migrations = store.list_migrations()?;

    if migrations.is_empty() {
        writeln!(writer, "No migrations to squash.")?;
        return Ok(EXIT_OK);
    }

    let result = squash_migrations(source_path, &migrations)?;

    let squash_name = name.map(|n| n.to_owned()).unwrap_or_else(|| {
        format!(
            "squashed-{}-to-{}",
            migrations.first().map(|m| m.sequence).unwrap_or(0),
            migrations.last().map(|m| m.sequence).unwrap_or(0)
        )
    });

    if dry_run {
        match format {
            OutputFormat::Human => {
                writeln!(writer, "--- Dry run: squash not saved ---")?;
                writeln!(writer, "Name: {squash_name}")?;
                writeln!(
                    writer,
                    "Squashing {} migrations: {}",
                    result.squashed_migration_names.len(),
                    result.squashed_migration_names.join(", ")
                )?;
                writeln!(
                    writer,
                    "Affected tables: {}",
                    if result.affected_tables.is_empty() {
                        "(none)".to_owned()
                    } else {
                        result.affected_tables.join(", ")
                    }
                )?;
                writeln!(writer)?;
                writeln!(writer, "--- SQUASHED UP SQL ---")?;
                write!(writer, "{}", result.up_sql)?;
                if let Some(ref down) = result.down_sql {
                    writeln!(writer)?;
                    writeln!(writer, "--- SQUASHED DOWN SQL ---")?;
                    write!(writer, "{down}")?;
                }
                writeln!(writer)?;
            }
            OutputFormat::Json => {
                #[derive(serde::Serialize)]
                struct SquashDryRun {
                    name: String,
                    up_sql: String,
                    down_sql: Option<String>,
                    affected_tables: Vec<String>,
                    squashed_migrations: Vec<String>,
                    dry_run: bool,
                }
                let output = SquashDryRun {
                    name: squash_name,
                    up_sql: result.up_sql,
                    down_sql: result.down_sql,
                    affected_tables: result.affected_tables,
                    squashed_migrations: result.squashed_migration_names,
                    dry_run: true,
                };
                let json = serde_json::to_string_pretty(&output)?;
                writeln!(writer, "{json}")?;
            }
        }
        return Ok(EXIT_OK);
    }

    // Delete old migrations and save the squashed one
    for migration in &migrations {
        store.delete_migration(&migration.id)?;
    }

    let source_str = source_path.to_string_lossy().into_owned();
    let squash_desc = format!(
        "Squashed from: {}",
        result.squashed_migration_names.join(", ")
    );
    let migration = store.save_migration(&crate::db::migration::NewMigration {
        name: &squash_name,
        up_sql: &result.up_sql,
        down_sql: result.down_sql.as_deref(),
        source_path: &source_str,
        target_path: &source_str,
        affected_tables: &result.affected_tables,
        description: Some(&squash_desc),
    })?;

    match format {
        OutputFormat::Human => {
            writeln!(
                writer,
                "Squashed {} migrations into '{}' ({})",
                result.squashed_migration_names.len(),
                migration.name,
                migration.id
            )?;
            writeln!(
                writer,
                "Affected tables: {}",
                if migration.affected_tables.is_empty() {
                    "(none)".to_owned()
                } else {
                    migration.affected_tables.join(", ")
                }
            )?;
        }
        OutputFormat::Json => {
            let json = serde_json::to_string_pretty(&migration)?;
            writeln!(writer, "{json}")?;
        }
    }

    Ok(EXIT_OK)
}

/// Runs `migrate conflicts`: checks for conflicts between stored migrations.
pub fn run_migrate_conflicts<W: Write>(writer: &mut W, format: OutputFormat) -> Result<i32> {
    let store = MigrationStore::new_default()?;
    let conflicts = store.detect_conflicts()?;

    match format {
        OutputFormat::Human => {
            if conflicts.is_empty() {
                writeln!(writer, "No migration conflicts detected.")?;
            } else {
                writeln!(
                    writer,
                    "Found {} migration conflict{}:",
                    conflicts.len(),
                    if conflicts.len() == 1 { "" } else { "s" }
                )?;
                for conflict in &conflicts {
                    writeln!(writer, "  ⚠ {}", conflict.description)?;
                }
            }
        }
        OutputFormat::Json => {
            let json = serde_json::to_string_pretty(&conflicts)?;
            writeln!(writer, "{json}")?;
        }
    }

    Ok(if conflicts.is_empty() {
        EXIT_OK
    } else {
        EXIT_DIFF
    })
}

// --- Human-readable output helpers ---

fn write_inspect_human<W: Write>(writer: &mut W, summary: &DatabaseSummary) -> Result<()> {
    writeln!(writer, "Database: {}", summary.path)?;
    writeln!(writer)?;

    if summary.tables.is_empty() {
        writeln!(writer, "No tables.")?;
    } else {
        writeln!(writer, "Tables ({}):", summary.tables.len())?;
        for table in &summary.tables {
            writeln!(
                writer,
                "  {:<30} {:>8} rows  {:>3} columns",
                table.name,
                table.row_count,
                table.columns.len()
            )?;
            for column in &table.columns {
                let pk_marker = if column.is_primary_key { " PK" } else { "" };
                let null_marker = if column.nullable {
                    " NULL"
                } else {
                    " NOT NULL"
                };
                writeln!(
                    writer,
                    "    {:<26} {:<12}{}{}",
                    column.name, column.col_type, null_marker, pk_marker
                )?;
            }
        }
    }

    if !summary.views.is_empty() {
        writeln!(writer)?;
        writeln!(writer, "Views ({}):", summary.views.len())?;
        for view in &summary.views {
            writeln!(writer, "  {}", view.name)?;
        }
    }

    if !summary.indexes.is_empty() {
        writeln!(writer)?;
        writeln!(writer, "Indexes ({}):", summary.indexes.len())?;
        for index in &summary.indexes {
            writeln!(writer, "  {} on {}", index.name, index.table_name)?;
        }
    }

    if !summary.triggers.is_empty() {
        writeln!(writer)?;
        writeln!(writer, "Triggers ({}):", summary.triggers.len())?;
        for trigger in &summary.triggers {
            writeln!(writer, "  {} on {}", trigger.name, trigger.table_name)?;
        }
    }

    Ok(())
}

fn write_inspect_json<W: Write>(writer: &mut W, summary: &DatabaseSummary) -> Result<()> {
    let json = serde_json::to_string_pretty(summary)?;
    writeln!(writer, "{json}")?;
    Ok(())
}

fn write_diff_human<W: Write>(
    writer: &mut W,
    schema: &SchemaDiff,
    data_diffs: &[TableDataDiff],
    summary: &DiffSummary,
    semantic_changes: &[SemanticChange],
) -> Result<()> {
    let mut any_output = false;

    // Semantic changes (renames, compatible type shifts)
    if !semantic_changes.is_empty() {
        writeln!(writer, "Semantic analysis:")?;
        for change in semantic_changes {
            match change {
                SemanticChange::TableRename {
                    left_name,
                    right_name,
                    confidence,
                } => {
                    writeln!(
                        writer,
                        "  ⟳ table rename: {} → {} (confidence: {}%)",
                        left_name, right_name, confidence
                    )?;
                }
                SemanticChange::ColumnRename {
                    table_name,
                    left_column,
                    right_column,
                    confidence,
                } => {
                    writeln!(
                        writer,
                        "  ⟳ column rename in {}: {} → {} (confidence: {}%)",
                        table_name, left_column, right_column, confidence
                    )?;
                }
                SemanticChange::CompatibleTypeShift {
                    table_name,
                    column_name,
                    left_type,
                    right_type,
                } => {
                    writeln!(
                        writer,
                        "  ≈ compatible type shift in {}.{}: {} → {}",
                        table_name, column_name, left_type, right_type
                    )?;
                }
            }
        }
        writeln!(writer)?;
        any_output = true;
    }

    // Schema changes
    if !schema.added_tables.is_empty() {
        for table in &schema.added_tables {
            writeln!(writer, "+ table {}", table.name)?;
        }
        any_output = true;
    }

    if !schema.removed_tables.is_empty() {
        for table in &schema.removed_tables {
            writeln!(writer, "- table {}", table.name)?;
        }
        any_output = true;
    }

    if !schema.modified_tables.is_empty() {
        for table_diff in &schema.modified_tables {
            writeln!(writer, "~ table {}", table_diff.table_name)?;
            for col in &table_diff.added_columns {
                writeln!(writer, "    + column {} {}", col.name, col.col_type)?;
            }
            for col in &table_diff.removed_columns {
                writeln!(writer, "    - column {} {}", col.name, col.col_type)?;
            }
            for (old, new) in &table_diff.modified_columns {
                writeln!(
                    writer,
                    "    ~ column {} ({} -> {})",
                    old.name, old.col_type, new.col_type
                )?;
            }
        }
        any_output = true;
    }

    if !schema.added_indexes.is_empty() {
        for index in &schema.added_indexes {
            writeln!(writer, "+ index {} on {}", index.name, index.table_name)?;
        }
        any_output = true;
    }

    if !schema.removed_indexes.is_empty() {
        for index in &schema.removed_indexes {
            writeln!(writer, "- index {} on {}", index.name, index.table_name)?;
        }
        any_output = true;
    }

    if !schema.modified_indexes.is_empty() {
        for (_, right) in &schema.modified_indexes {
            writeln!(writer, "~ index {} on {}", right.name, right.table_name)?;
        }
        any_output = true;
    }

    if !schema.added_triggers.is_empty() {
        for trigger in &schema.added_triggers {
            writeln!(
                writer,
                "+ trigger {} on {}",
                trigger.name, trigger.table_name
            )?;
        }
        any_output = true;
    }

    if !schema.removed_triggers.is_empty() {
        for trigger in &schema.removed_triggers {
            writeln!(
                writer,
                "- trigger {} on {}",
                trigger.name, trigger.table_name
            )?;
        }
        any_output = true;
    }

    if !schema.modified_triggers.is_empty() {
        for (_, right) in &schema.modified_triggers {
            writeln!(writer, "~ trigger {} on {}", right.name, right.table_name)?;
        }
        any_output = true;
    }

    // Data changes
    for diff in data_diffs {
        let stats = &diff.stats;
        if stats.added == 0 && stats.removed == 0 && stats.modified == 0 {
            continue;
        }
        if any_output {
            writeln!(writer)?;
        }
        writeln!(
            writer,
            "table {}: +{} added, -{} removed, ~{} modified, {} unchanged",
            diff.table_name, stats.added, stats.removed, stats.modified, stats.unchanged
        )?;

        // Column-level detail for modified rows
        for row_mod in &diff.modified_rows {
            let pk = row_mod
                .primary_key
                .iter()
                .map(|v| v.display())
                .collect::<Vec<_>>()
                .join(", ");
            let changes = row_mod
                .changes
                .iter()
                .map(|c| {
                    format!(
                        "{}: {} → {}",
                        c.column,
                        c.old_value.display(),
                        c.new_value.display()
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            writeln!(writer, "  [{}] {}", pk, changes)?;
        }
        any_output = true;
    }

    // Summary
    if any_output && (summary.tables_compared > 0 || summary.tables_added > 0) {
        writeln!(writer)?;
        writeln!(
            writer,
            "Summary: {} tables compared, {} changed, +{} added, -{} removed, ~{} modified rows, {} cells changed",
            summary.tables_compared,
            summary.tables_changed,
            summary.total_rows_added,
            summary.total_rows_removed,
            summary.total_rows_modified,
            summary.total_cells_changed,
        )?;
    }

    if !any_output {
        writeln!(writer, "No differences.")?;
    }

    Ok(())
}

fn write_diff_json<W: Write>(
    writer: &mut W,
    schema: &SchemaDiff,
    data_diffs: &[TableDataDiff],
    summary: &DiffSummary,
    semantic_changes: &[SemanticChange],
) -> Result<()> {
    #[derive(serde::Serialize)]
    struct DiffOutput<'a> {
        schema: &'a SchemaDiff,
        data: &'a [TableDataDiff],
        summary: &'a DiffSummary,
        semantic_changes: &'a [SemanticChange],
    }

    let output = DiffOutput {
        schema,
        data: data_diffs,
        summary,
        semantic_changes,
    };
    let json = serde_json::to_string_pretty(&output)?;
    writeln!(writer, "{json}")?;
    Ok(())
}

fn write_merge_human<W: Write>(writer: &mut W, result: &ThreeWayMergeResult) -> Result<()> {
    let summary = &result.summary;

    writeln!(
        writer,
        "Three-way merge: {} tables resolved, {} tables with conflicts",
        summary.tables_resolved, summary.tables_conflicted
    )?;

    if !result.resolved_tables.is_empty() {
        writeln!(writer)?;
        writeln!(writer, "Resolved tables:")?;
        for table in &result.resolved_tables {
            let source_label = match table.source {
                MergeSource::Left => "left only",
                MergeSource::Right => "right only",
                MergeSource::Both => "both sides",
                MergeSource::Neither => "unchanged",
            };
            let changes = table.row_changes.added_rows.len()
                + table.row_changes.removed_row_keys.len()
                + table.row_changes.modified_rows.len();
            if changes > 0 {
                writeln!(
                    writer,
                    "  ✓ {} (source: {}, {} row changes)",
                    table.table_name, source_label, changes
                )?;
            } else {
                writeln!(
                    writer,
                    "  ✓ {} (source: {})",
                    table.table_name, source_label
                )?;
            }
        }
    }

    if !result.conflicts.is_empty() {
        writeln!(writer)?;
        writeln!(writer, "Conflicts:")?;
        for conflict in &result.conflicts {
            match &conflict.kind {
                ConflictKind::SchemaConflict { .. } => {
                    writeln!(
                        writer,
                        "  ✗ {} — schema conflict (both sides modified differently)",
                        conflict.table_name
                    )?;
                }
                ConflictKind::RowConflict { primary_key, .. } => {
                    let pk = primary_key
                        .iter()
                        .map(|v| v.display())
                        .collect::<Vec<_>>()
                        .join(", ");
                    writeln!(
                        writer,
                        "  ✗ {} [{}] — row conflict (both sides changed same row differently)",
                        conflict.table_name, pk
                    )?;
                }
                ConflictKind::DeleteModifyConflict {
                    primary_key,
                    deleted_by,
                    ..
                } => {
                    let pk = primary_key
                        .iter()
                        .map(|v| v.display())
                        .collect::<Vec<_>>()
                        .join(", ");
                    let deleter = match deleted_by {
                        MergeSource::Left => "left",
                        MergeSource::Right => "right",
                        _ => "unknown",
                    };
                    writeln!(
                        writer,
                        "  ✗ {} [{}] — delete/modify conflict ({} deleted, other modified)",
                        conflict.table_name, pk, deleter
                    )?;
                }
                ConflictKind::TableDeleteConflict { deleted_by } => {
                    let deleter = match deleted_by {
                        MergeSource::Left => "left",
                        MergeSource::Right => "right",
                        _ => "unknown",
                    };
                    writeln!(
                        writer,
                        "  ✗ {} — table deleted by {} while modified by other side",
                        conflict.table_name, deleter
                    )?;
                }
            }
        }
    }

    if summary.row_conflicts > 0 || summary.schema_conflicts > 0 {
        writeln!(writer)?;
        writeln!(
            writer,
            "Total: {} row conflicts, {} schema conflicts",
            summary.row_conflicts, summary.schema_conflicts
        )?;
    }

    Ok(())
}

fn write_merge_json<W: Write>(writer: &mut W, result: &ThreeWayMergeResult) -> Result<()> {
    let json = serde_json::to_string_pretty(result)?;
    writeln!(writer, "{json}")?;
    Ok(())
}

fn has_any_changes(schema: &SchemaDiff, data_diffs: &[TableDataDiff]) -> bool {
    !schema.added_tables.is_empty()
        || !schema.removed_tables.is_empty()
        || !schema.modified_tables.is_empty()
        || !schema.added_indexes.is_empty()
        || !schema.removed_indexes.is_empty()
        || !schema.modified_indexes.is_empty()
        || !schema.added_triggers.is_empty()
        || !schema.removed_triggers.is_empty()
        || !schema.modified_triggers.is_empty()
        || data_diffs
            .iter()
            .any(|diff| diff.stats.added > 0 || diff.stats.removed > 0 || diff.stats.modified > 0)
}

fn write_migrate_list_human<W: Write>(
    writer: &mut W,
    migrations: &[Migration],
    summary: &MigrationChainSummary,
) -> Result<()> {
    if migrations.is_empty() {
        writeln!(writer, "No migrations stored.")?;
        return Ok(());
    }

    writeln!(
        writer,
        "{:<6} {:<38} {:<30} {:<10} {:<10} CREATED",
        "SEQ", "ID", "NAME", "VALIDATED", "REVERT"
    )?;
    for migration in migrations {
        writeln!(
            writer,
            "{:<6} {:<38} {:<30} {:<10} {:<10} {}",
            migration.sequence,
            migration.id,
            truncate(&migration.name, 28),
            if migration.validated { "✓" } else { "-" },
            if migration.down_sql.is_some() {
                "✓"
            } else {
                "-"
            },
            &migration.created_at[..19.min(migration.created_at.len())],
        )?;
    }
    writeln!(writer)?;
    writeln!(
        writer,
        "Chain: {} migration{}, {} validated, {} reversible",
        summary.total_migrations,
        if summary.total_migrations == 1 {
            ""
        } else {
            "s"
        },
        summary.validated_count,
        summary.reversible_count,
    )?;

    Ok(())
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_owned()
    } else {
        format!("{}…", &s[..max_len.saturating_sub(1)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use tempfile::TempDir;

    fn create_test_db(dir: &TempDir, name: &str, sql: &str) -> std::path::PathBuf {
        let path = dir.path().join(name);
        let conn = Connection::open(&path).expect("create db");
        conn.execute_batch(sql).expect("execute sql");
        path
    }

    #[test]
    fn inspect_human_includes_table_name_and_columns() {
        let dir = TempDir::new().expect("temp dir");
        let db = create_test_db(
            &dir,
            "test.db",
            "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT NOT NULL);
             INSERT INTO items VALUES (1, 'a'), (2, 'b');",
        );

        let mut output = Vec::new();
        let code = run_inspect(&mut output, &db, OutputFormat::Human).expect("inspect");
        let text = String::from_utf8(output).expect("utf8");

        assert_eq!(code, EXIT_OK);
        assert!(text.contains("items"));
        assert!(text.contains("2 rows"));
        assert!(text.contains("id"));
        assert!(text.contains("name"));
    }

    #[test]
    fn inspect_json_produces_valid_json() {
        let dir = TempDir::new().expect("temp dir");
        let db = create_test_db(
            &dir,
            "test.db",
            "CREATE TABLE items (id INTEGER PRIMARY KEY);",
        );

        let mut output = Vec::new();
        run_inspect(&mut output, &db, OutputFormat::Json).expect("inspect");
        let text = String::from_utf8(output).expect("utf8");

        let parsed: serde_json::Value = serde_json::from_str(&text).expect("valid json");
        assert!(parsed.get("tables").is_some());
    }

    #[test]
    fn diff_returns_exit_diff_when_changes_exist() {
        let dir = TempDir::new().expect("temp dir");
        let left = create_test_db(
            &dir,
            "left.db",
            "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);
             INSERT INTO items VALUES (1, 'a');",
        );
        let right = create_test_db(
            &dir,
            "right.db",
            "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);
             INSERT INTO items VALUES (1, 'b');",
        );

        let mut output = Vec::new();
        let code = run_diff(&mut output, &left, &right, OutputFormat::Human).expect("diff");
        assert_eq!(code, EXIT_DIFF);
    }

    #[test]
    fn diff_returns_exit_ok_when_identical() {
        let dir = TempDir::new().expect("temp dir");
        let left = create_test_db(
            &dir,
            "left.db",
            "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);
             INSERT INTO items VALUES (1, 'a');",
        );
        let right = create_test_db(
            &dir,
            "right.db",
            "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);
             INSERT INTO items VALUES (1, 'a');",
        );

        let mut output = Vec::new();
        let code = run_diff(&mut output, &left, &right, OutputFormat::Human).expect("diff");
        let text = String::from_utf8(output).expect("utf8");

        assert_eq!(code, EXIT_OK);
        assert!(text.contains("No differences."));
    }

    #[test]
    fn export_produces_valid_sql() {
        let dir = TempDir::new().expect("temp dir");
        let left = create_test_db(
            &dir,
            "left.db",
            "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);
             INSERT INTO items VALUES (1, 'a');",
        );
        let right = create_test_db(
            &dir,
            "right.db",
            "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);
             INSERT INTO items VALUES (1, 'b'), (2, 'c');",
        );

        let mut output = Vec::new();
        let code = run_export(&mut output, &left, &right).expect("export");
        let text = String::from_utf8(output).expect("utf8");

        assert_eq!(code, EXIT_OK);
        assert!(text.contains("BEGIN TRANSACTION;"));
        assert!(text.contains("COMMIT;"));
    }

    #[test]
    fn diff_json_produces_valid_json_with_schema_and_data() {
        let dir = TempDir::new().expect("temp dir");
        let left = create_test_db(
            &dir,
            "left.db",
            "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);
             INSERT INTO items VALUES (1, 'a');",
        );
        let right = create_test_db(
            &dir,
            "right.db",
            "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);
             INSERT INTO items VALUES (1, 'b');",
        );

        let mut output = Vec::new();
        run_diff(&mut output, &left, &right, OutputFormat::Json).expect("diff");
        let text = String::from_utf8(output).expect("utf8");

        let parsed: serde_json::Value = serde_json::from_str(&text).expect("valid json");
        assert!(parsed.get("schema").is_some());
        assert!(parsed.get("data").is_some());
    }
}
