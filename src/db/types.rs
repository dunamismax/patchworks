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

/// Metadata about a SQLite index or trigger.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchemaObjectInfo {
    /// Schema object name.
    pub name: String,
    /// Table this object is attached to.
    pub table_name: String,
    /// Original `CREATE ...` SQL, if available.
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
#[derive(Clone, Debug, Serialize, Deserialize)]
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

impl PartialEq for SqlValue {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Null, Self::Null) => true,
            (Self::Integer(a), Self::Integer(b)) => a == b,
            (Self::Real(a), Self::Real(b)) => a.to_bits() == b.to_bits(),
            (Self::Text(a), Self::Text(b)) => a == b,
            (Self::Blob(a), Self::Blob(b)) => a == b,
            _ => false,
        }
    }
}

impl Eq for SqlValue {}

impl PartialOrd for SqlValue {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SqlValue {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        use std::cmp::Ordering;

        let rank = |v: &SqlValue| -> u8 {
            match v {
                SqlValue::Null => 0,
                SqlValue::Integer(_) => 1,
                SqlValue::Real(_) => 2,
                SqlValue::Text(_) => 3,
                SqlValue::Blob(_) => 4,
            }
        };

        let rank_ord = rank(self).cmp(&rank(other));
        if rank_ord != Ordering::Equal {
            return rank_ord;
        }

        match (self, other) {
            (Self::Null, Self::Null) => Ordering::Equal,
            (Self::Integer(a), Self::Integer(b)) => a.cmp(b),
            (Self::Real(a), Self::Real(b)) => a
                .partial_cmp(b)
                .unwrap_or_else(|| a.to_bits().cmp(&b.to_bits())),
            (Self::Text(a), Self::Text(b)) => a.cmp(b),
            (Self::Blob(a), Self::Blob(b)) => a.cmp(b),
            _ => Ordering::Equal,
        }
    }
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
    /// Discovered indexes.
    pub indexes: Vec<SchemaObjectInfo>,
    /// Discovered triggers.
    pub triggers: Vec<SchemaObjectInfo>,
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
    /// Indexes only present on the right side.
    pub added_indexes: Vec<SchemaObjectInfo>,
    /// Indexes only present on the left side.
    pub removed_indexes: Vec<SchemaObjectInfo>,
    /// Indexes present on both sides but with different definitions.
    pub modified_indexes: Vec<(SchemaObjectInfo, SchemaObjectInfo)>,
    /// Triggers only present on the right side.
    pub added_triggers: Vec<SchemaObjectInfo>,
    /// Triggers only present on the left side.
    pub removed_triggers: Vec<SchemaObjectInfo>,
    /// Triggers present on both sides but with different definitions.
    pub modified_triggers: Vec<(SchemaObjectInfo, SchemaObjectInfo)>,
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

/// Aggregate summary across all tables in a diff.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffSummary {
    /// Total tables compared.
    pub tables_compared: usize,
    /// Tables with changes.
    pub tables_changed: usize,
    /// Tables with no changes.
    pub tables_unchanged: usize,
    /// Schema-only changes: tables added.
    pub tables_added: usize,
    /// Schema-only changes: tables removed.
    pub tables_removed: usize,
    /// Schema-only changes: tables with schema modifications.
    pub tables_schema_modified: usize,
    /// Aggregate row stats across all tables.
    pub total_rows_added: u64,
    /// Aggregate removed rows across all tables.
    pub total_rows_removed: u64,
    /// Aggregate modified rows across all tables.
    pub total_rows_modified: u64,
    /// Aggregate unchanged rows across all tables.
    pub total_rows_unchanged: u64,
    /// Count of columns changed across all modified rows.
    pub total_cells_changed: u64,
    /// Index changes.
    pub indexes_added: usize,
    /// Indexes removed.
    pub indexes_removed: usize,
    /// Indexes modified.
    pub indexes_modified: usize,
    /// Trigger changes.
    pub triggers_added: usize,
    /// Triggers removed.
    pub triggers_removed: usize,
    /// Triggers modified.
    pub triggers_modified: usize,
}

/// Filter criteria for narrowing diff results.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffFilter {
    /// If non-empty, only include these table names.
    pub tables: Vec<String>,
    /// Show added rows.
    pub show_added: bool,
    /// Show removed rows.
    pub show_removed: bool,
    /// Show modified rows.
    pub show_modified: bool,
    /// Show unchanged rows (typically false).
    pub show_unchanged: bool,
}

impl DiffFilter {
    /// A filter that shows all change types for all tables.
    pub fn all() -> Self {
        Self {
            tables: Vec::new(),
            show_added: true,
            show_removed: true,
            show_modified: true,
            show_unchanged: false,
        }
    }

    /// Returns true if the filter accepts this table.
    pub fn accepts_table(&self, table_name: &str) -> bool {
        self.tables.is_empty() || self.tables.iter().any(|name| name == table_name)
    }
}

/// A detected semantic change beyond raw structural diff.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SemanticChange {
    /// A table appears to have been renamed (left name, right name, confidence score 0-100).
    TableRename {
        /// Original table name on the left side.
        left_name: String,
        /// New table name on the right side.
        right_name: String,
        /// Confidence score (0-100) based on column similarity.
        confidence: u8,
    },
    /// A column appears to have been renamed within a table.
    ColumnRename {
        /// Table this column belongs to.
        table_name: String,
        /// Original column name.
        left_column: String,
        /// New column name.
        right_column: String,
        /// Confidence score (0-100).
        confidence: u8,
    },
    /// A column type changed in a way that is likely compatible.
    CompatibleTypeShift {
        /// Table this column belongs to.
        table_name: String,
        /// Column name.
        column_name: String,
        /// Original type.
        left_type: String,
        /// New type.
        right_type: String,
    },
}

/// Result of a three-way merge between an ancestor and two derived databases.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ThreeWayMergeResult {
    /// Tables where both sides agree or only one side changed (auto-mergeable).
    pub resolved_tables: Vec<MergedTable>,
    /// Tables where both sides changed the same rows/schema incompatibly.
    pub conflicts: Vec<MergeConflict>,
    /// Summary of the merge result.
    pub summary: MergeSummary,
}

/// A table that was successfully merged without conflicts.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MergedTable {
    /// Table name.
    pub table_name: String,
    /// Which side(s) contributed changes.
    pub source: MergeSource,
    /// Schema changes from the contributing side, if any.
    pub schema_changes: Option<TableSchemaDiff>,
    /// Row-level changes to apply.
    pub row_changes: MergedRowChanges,
}

/// Which side of a three-way merge contributed a change.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MergeSource {
    /// Only the left side changed relative to the ancestor.
    Left,
    /// Only the right side changed relative to the ancestor.
    Right,
    /// Both sides changed, but in compatible ways.
    Both,
    /// Neither side changed.
    Neither,
}

/// Non-conflicting row-level changes from a merge.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MergedRowChanges {
    /// Rows to add (from whichever side added them).
    pub added_rows: Vec<Vec<SqlValue>>,
    /// Primary keys of rows to remove.
    pub removed_row_keys: Vec<Vec<SqlValue>>,
    /// Rows with non-conflicting modifications.
    pub modified_rows: Vec<RowModification>,
}

/// A conflict detected during three-way merge.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MergeConflict {
    /// Table where the conflict occurs.
    pub table_name: String,
    /// The kind of conflict.
    pub kind: ConflictKind,
}

/// The specific nature of a merge conflict.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ConflictKind {
    /// Both sides modified the schema of the same table differently.
    SchemaConflict {
        /// Left-side schema changes.
        left_changes: TableSchemaDiff,
        /// Right-side schema changes.
        right_changes: TableSchemaDiff,
    },
    /// Both sides modified the same row with different values.
    RowConflict {
        /// Primary key of the conflicting row.
        primary_key: Vec<SqlValue>,
        /// Left-side cell changes.
        left_changes: Vec<CellChange>,
        /// Right-side cell changes.
        right_changes: Vec<CellChange>,
    },
    /// One side removed a row that the other side modified.
    DeleteModifyConflict {
        /// Primary key of the conflicting row.
        primary_key: Vec<SqlValue>,
        /// Which side deleted the row.
        deleted_by: MergeSource,
        /// Changes from the side that modified it.
        modifications: Vec<CellChange>,
    },
    /// One side removed the table while the other modified it.
    TableDeleteConflict {
        /// Which side deleted the table.
        deleted_by: MergeSource,
    },
}

/// Summary statistics for a three-way merge.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MergeSummary {
    /// Tables that auto-merged cleanly.
    pub tables_resolved: usize,
    /// Tables with conflicts requiring manual resolution.
    pub tables_conflicted: usize,
    /// Total row-level conflicts.
    pub row_conflicts: usize,
    /// Total schema-level conflicts.
    pub schema_conflicts: usize,
}

/// An annotation attached to a diff entry for triage workflows.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffAnnotation {
    /// Which table this annotation applies to.
    pub table_name: String,
    /// Optional primary key of the specific row, or `None` for table-level annotations.
    pub row_key: Option<Vec<SqlValue>>,
    /// The triage status.
    pub status: AnnotationStatus,
    /// Free-text note.
    pub note: String,
}

/// Triage status for a diff annotation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnnotationStatus {
    /// Not yet reviewed.
    Pending,
    /// Reviewed and approved.
    Approved,
    /// Reviewed and rejected — should be reverted or addressed.
    Rejected,
    /// Needs further discussion before a decision.
    NeedsDiscussion,
    /// Acknowledged but deferred.
    Deferred,
}

impl std::fmt::Display for AnnotationStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::Approved => write!(f, "approved"),
            Self::Rejected => write!(f, "rejected"),
            Self::NeedsDiscussion => write!(f, "needs-discussion"),
            Self::Deferred => write!(f, "deferred"),
        }
    }
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
