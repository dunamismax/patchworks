//! Workspace state used by the egui application.

use std::path::PathBuf;

use crate::db::differ::DatabaseDiff;
use crate::db::types::{DatabaseSummary, Snapshot, TablePage, TableQuery};

/// Which main view is visible in the workspace.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkspaceView {
    /// Inspect a selected table.
    Table,
    /// Browse full schema DDL (tables, views, indexes, triggers).
    SchemaBrowser,
    /// Show row-level diffs.
    Diff,
    /// Show schema changes.
    SchemaDiff,
    /// Show snapshot history.
    Snapshots,
    /// Show SQL export text.
    SqlExport,
}

/// User theme preference.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemePreference {
    /// Follow the operating system setting.
    System,
    /// Always use the dark theme.
    Dark,
    /// Always use the light theme.
    Light,
}

/// Rendering style for diff results.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiffDisplayMode {
    /// Tabular grid of added, removed, and modified rows.
    Grid,
    /// Unified list of changes.
    Unified,
}

/// Coarse progress state for background work shown in the UI.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProgressState {
    /// Human-readable description of the active step.
    pub label: String,
    /// Number of fully completed steps before the current step.
    pub completed_steps: usize,
    /// Total step count when known.
    pub total_steps: Option<usize>,
}

impl ProgressState {
    /// Creates a new progress snapshot.
    pub fn new(
        label: impl Into<String>,
        completed_steps: usize,
        total_steps: Option<usize>,
    ) -> Self {
        Self {
            label: label.into(),
            completed_steps,
            total_steps,
        }
    }

    /// Returns a best-effort fractional completion for determinate progress.
    pub fn fraction(&self) -> Option<f32> {
        self.total_steps.map(|total| {
            if total == 0 {
                0.0
            } else {
                (self.completed_steps.min(total) as f32) / (total as f32)
            }
        })
    }

    /// Returns a short step-count label when the total is known.
    pub fn step_label(&self) -> Option<String> {
        self.total_steps.map(|total| {
            format!(
                "Step {} of {}",
                (self.completed_steps + 1).min(total),
                total
            )
        })
    }
}

/// State for a single database pane.
#[derive(Clone, Debug, Default)]
pub struct DatabasePaneState {
    /// Loaded database path.
    pub path: Option<PathBuf>,
    /// Whether a database inspection is currently running in the background.
    pub is_loading: bool,
    /// Last schema summary.
    pub summary: Option<DatabaseSummary>,
    /// Currently selected table.
    pub selected_table: Option<String>,
    /// Whether the visible table page is currently refreshing in the background.
    pub is_loading_table: bool,
    /// Progress for the active background load on this pane, if any.
    pub progress: Option<ProgressState>,
    /// Currently loaded page of table rows.
    pub table_page: Option<TablePage>,
    /// Query settings for the table page.
    pub table_query: TableQuery,
    /// Snapshots associated with the current source database.
    pub snapshots: Vec<Snapshot>,
    /// Last visible error for this pane.
    pub error: Option<String>,
    /// Filter text for the table list in the file panel.
    pub table_filter: String,
}

/// State for the active diff result.
#[derive(Clone, Debug)]
pub struct DiffState {
    /// Latest diff payload.
    pub result: Option<DatabaseDiff>,
    /// Whether a background diff computation is currently running.
    pub is_computing: bool,
    /// Progress for the active background diff, if any.
    pub progress: Option<ProgressState>,
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
            is_computing: false,
            progress: None,
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
    /// User theme preference.
    pub theme: ThemePreference,
    /// Recently opened database file paths (most recent first).
    pub recent_files: Vec<PathBuf>,
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
            theme: ThemePreference::System,
            recent_files: Vec::new(),
        }
    }
}
