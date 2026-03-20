//! Workspace state used by the egui application.

use std::path::PathBuf;

use crate::db::differ::DatabaseDiff;
use crate::db::types::{DatabaseSummary, Snapshot, TablePage, TableQuery};

/// Which main view is visible in the workspace.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkspaceView {
    /// Inspect a selected table.
    Table,
    /// Show row-level diffs.
    Diff,
    /// Show schema changes.
    SchemaDiff,
    /// Show snapshot history.
    Snapshots,
    /// Show SQL export text.
    SqlExport,
}

/// Rendering style for diff results.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiffDisplayMode {
    /// Tabular grid of added, removed, and modified rows.
    Grid,
    /// Unified list of changes.
    Unified,
}

/// State for a single database pane.
#[derive(Clone, Debug, Default)]
pub struct DatabasePaneState {
    /// Loaded database path.
    pub path: Option<PathBuf>,
    /// Last schema summary.
    pub summary: Option<DatabaseSummary>,
    /// Currently selected table.
    pub selected_table: Option<String>,
    /// Currently loaded page of table rows.
    pub table_page: Option<TablePage>,
    /// Query settings for the table page.
    pub table_query: TableQuery,
    /// Snapshots associated with the current source database.
    pub snapshots: Vec<Snapshot>,
    /// Last visible error for this pane.
    pub error: Option<String>,
}

/// State for the active diff result.
#[derive(Clone, Debug)]
pub struct DiffState {
    /// Latest diff payload.
    pub result: Option<DatabaseDiff>,
    /// Selected table diff.
    pub selected_table: Option<String>,
    /// Chosen rendering mode.
    pub display_mode: DiffDisplayMode,
    /// Last diff error.
    pub error: Option<String>,
}

impl Default for DiffState {
    fn default() -> Self {
        Self {
            result: None,
            selected_table: None,
            display_mode: DiffDisplayMode::Grid,
            error: None,
        }
    }
}

/// Entire workspace state shared by the desktop app.
#[derive(Debug)]
pub struct WorkspaceState {
    /// Left database pane.
    pub left: DatabasePaneState,
    /// Right database pane.
    pub right: DatabasePaneState,
    /// Active central workspace view.
    pub active_view: WorkspaceView,
    /// Current diff state.
    pub diff: DiffState,
    /// Draft snapshot name.
    pub snapshot_name: String,
    /// Last status line message.
    pub status_message: Option<String>,
}

impl Default for WorkspaceState {
    fn default() -> Self {
        Self {
            left: DatabasePaneState::default(),
            right: DatabasePaneState::default(),
            active_view: WorkspaceView::Table,
            diff: DiffState::default(),
            snapshot_name: "Snapshot".to_owned(),
            status_message: None,
        }
    }
}
