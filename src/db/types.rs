//! Core data types used across Patchworks.

use serde::{Deserialize, Serialize};

/// Metadata about a SQLite table.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TableInfo {
    /// Table name.
    pub name: String,
    /// Column metadata in declaration order.
    pub columns: Vec<ColumnInfo>,
    /// Total number of rows in the table.
    pub row_count: u64,
    /// Column names that form the primary key.
    pub primary_key: Vec<String>,
    /// Original `CREATE TABLE` SQL, if available.
    pub create_sql: Option<String>,
}

/// Metadata about a SQLite view.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewInfo {
    /// View name.
    pub name: String,
    /// Original `CREATE VIEW` SQL, if available.
    pub create_sql: Option<String>,
}

/// Metadata about a SQLite column.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColumnInfo {
    /// Column name.
    pub name: String,
    /// SQLite declared type or inferred affinity label.
    pub col_type: String,
    /// Whether the column accepts `NULL`.
    pub nullable: bool,
    /// Column default expression if one exists.
    pub default_value: Option<String>,
    /// Whether the column participates in the primary key.
    pub is_primary_key: bool,
}

/// A generic SQLite value for display and comparison.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum SqlValue {
    /// SQL `NULL`.
    Null,
    /// Signed integer value.
    Integer(i64),
    /// Floating-point value.
    Real(f64),
    /// UTF-8 text value.
    Text(String),
    /// Binary value.
    Blob(Vec<u8>),
}

/// Full schema summary for a database file.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DatabaseSummary {
    /// Original database path.
    pub path: String,
    /// Discovered tables.
    pub tables: Vec<TableInfo>,
    /// Discovered views.
    pub views: Vec<ViewInfo>,
}

/// A paginated slice of table data.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TablePage {
    /// Table name being viewed.
    pub table_name: String,
    /// Column metadata for the rendered rows.
    pub columns: Vec<ColumnInfo>,
    /// Row values for the current page.
    pub rows: Vec<Vec<SqlValue>>,
    /// Zero-based page index.
    pub page: usize,
    /// Requested page size.
    pub page_size: usize,
    /// Total rows in the table.
    pub total_rows: u64,
    /// Applied sort, if any.
    pub sort: Option<TableSort>,
}

/// Sorting information for table reads.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableSort {
    /// Column name used for sorting.
    pub column: String,
    /// Sort direction.
    pub direction: SortDirection,
}

/// Sort direction for paginated table reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SortDirection {
    /// Ascending order.
    Asc,
    /// Descending order.
    Desc,
}

/// Input parameters for reading a paginated table page.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableQuery {
    /// Zero-based page index.
    pub page: usize,
    /// Maximum rows to return.
    pub page_size: usize,
    /// Optional sort selection.
    pub sort: Option<TableSort>,
}

impl Default for TableQuery {
    fn default() -> Self {
        Self {
            page: 0,
            page_size: 100,
            sort: None,
        }
    }
}

/// Schema-level diff between two databases.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SchemaDiff {
    /// Tables only present on the right side.
    pub added_tables: Vec<TableInfo>,
    /// Tables only present on the left side.
    pub removed_tables: Vec<TableInfo>,
    /// Tables present on both sides with column-level changes.
    pub modified_tables: Vec<TableSchemaDiff>,
    /// Tables whose schemas match exactly.
    pub unchanged_tables: Vec<String>,
}

/// Schema changes within a single table.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TableSchemaDiff {
    /// Table name.
    pub table_name: String,
    /// Columns only present on the right side.
    pub added_columns: Vec<ColumnInfo>,
    /// Columns only present on the left side.
    pub removed_columns: Vec<ColumnInfo>,
    /// Columns with the same name but different definitions.
    pub modified_columns: Vec<(ColumnInfo, ColumnInfo)>,
}

/// Row-level diff for a single table.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TableDataDiff {
    /// Table name.
    pub table_name: String,
    /// Column names used for row context.
    pub columns: Vec<String>,
    /// Right-side rows not found on the left side.
    pub added_rows: Vec<Vec<SqlValue>>,
    /// Left-side rows not found on the right side.
    pub removed_rows: Vec<Vec<SqlValue>>,
    /// Identity values for removed rows, aligned by index with `removed_rows`.
    pub removed_row_keys: Vec<Vec<SqlValue>>,
    /// Rows with matching identity but different cell values.
    pub modified_rows: Vec<RowModification>,
    /// Aggregate diff statistics.
    pub stats: DiffStats,
    /// User-facing warnings about diff reliability or scale.
    pub warnings: Vec<String>,
}

/// Changes to a single row.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RowModification {
    /// Primary key or synthetic row identity values.
    pub primary_key: Vec<SqlValue>,
    /// Per-cell modifications inside the row.
    pub changes: Vec<CellChange>,
}

/// Change to a single cell.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CellChange {
    /// Column name that changed.
    pub column: String,
    /// Left-side value.
    pub old_value: SqlValue,
    /// Right-side value.
    pub new_value: SqlValue,
}

/// Diff summary counts.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffStats {
    /// Total rows on the left side.
    pub total_rows_left: u64,
    /// Total rows on the right side.
    pub total_rows_right: u64,
    /// Count of added rows.
    pub added: u64,
    /// Count of removed rows.
    pub removed: u64,
    /// Count of modified rows.
    pub modified: u64,
    /// Count of unchanged rows.
    pub unchanged: u64,
}

/// A saved snapshot of a database state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    /// Snapshot UUID.
    pub id: String,
    /// Human-friendly snapshot name.
    pub name: String,
    /// Original source database file path.
    pub source_path: String,
    /// Snapshot creation timestamp in ISO 8601 form.
    pub created_at: String,
    /// Number of tables captured in the snapshot.
    pub table_count: u32,
    /// Total rows across all tables.
    pub total_rows: u64,
}

impl SqlValue {
    /// Returns a short display string suitable for table cells.
    pub fn display(&self) -> String {
        match self {
            Self::Null => "NULL".to_owned(),
            Self::Integer(value) => value.to_string(),
            Self::Real(value) => value.to_string(),
            Self::Text(value) => value.clone(),
            Self::Blob(bytes) => format!("[BLOB: {} bytes]", bytes.len()),
        }
    }
}
