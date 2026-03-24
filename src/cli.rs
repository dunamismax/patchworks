//! Headless CLI commands for Patchworks.
//!
//! Each public function corresponds to a CLI subcommand and writes its output to the provided
//! writer. All functions return an appropriate exit code: 0 for success, 1 for operational errors,
//! and 2 for diff results that found differences (enabling CI gate usage).

use std::io::Write;
use std::path::Path;

use crate::db::differ::diff_databases;
use crate::db::inspector::inspect_database;
use crate::db::snapshot::SnapshotStore;
use crate::db::types::{DatabaseSummary, SchemaDiff, TableDataDiff};
use crate::diff::export::write_export;
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
        OutputFormat::Human => write_diff_human(writer, &diff.schema, &diff.data_diffs)?,
        OutputFormat::Json => write_diff_json(writer, &diff.schema, &diff.data_diffs)?,
    }

    Ok(if has_changes { EXIT_DIFF } else { EXIT_OK })
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
) -> Result<()> {
    let mut any_output = false;

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
        any_output = true;
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
) -> Result<()> {
    #[derive(serde::Serialize)]
    struct DiffOutput<'a> {
        schema: &'a SchemaDiff,
        data: &'a [TableDataDiff],
    }

    let output = DiffOutput {
        schema,
        data: data_diffs,
    };
    let json = serde_json::to_string_pretty(&output)?;
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
