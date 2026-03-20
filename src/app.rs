//! Main egui application for Patchworks.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::Duration;

use eframe::egui;

use crate::db::differ::{diff_databases, DatabaseDiff};
use crate::db::inspector::{inspect_database, read_table_page_for_table};
use crate::db::snapshot::SnapshotStore;
use crate::error::Result;
use crate::state::workspace::{DatabasePaneState, WorkspaceState, WorkspaceView};
use crate::ui;

/// Startup options derived from CLI arguments.
#[derive(Clone, Debug, Default)]
pub struct StartupOptions {
    /// Left-side file to open on launch.
    pub left: Option<PathBuf>,
    /// Right-side file to open on launch.
    pub right: Option<PathBuf>,
}

/// Patchworks desktop application.
pub struct PatchworksApp {
    workspace: WorkspaceState,
    snapshot_store: SnapshotStore,
    running_diff: Option<RunningDiffTask>,
}

impl PatchworksApp {
    /// Creates the application and eagerly loads CLI-provided files.
    pub fn new(startup: StartupOptions) -> Result<Self> {
        let mut app = Self {
            workspace: WorkspaceState::default(),
            snapshot_store: SnapshotStore::new_default()?,
            running_diff: None,
        };
        if let Some(path) = startup.left {
            app.load_left(path)?;
        }
        if let Some(path) = startup.right {
            app.load_right(path)?;
            app.request_diff();
        }
        Ok(app)
    }

    fn load_left(&mut self, path: PathBuf) -> Result<()> {
        load_into_pane(&mut self.workspace.left, &self.snapshot_store, &path)?;
        self.clear_diff_state();
        self.workspace.status_message = Some(format!("Loaded {}", path.display()));
        Ok(())
    }

    fn load_right(&mut self, path: PathBuf) -> Result<()> {
        load_into_pane(&mut self.workspace.right, &self.snapshot_store, &path)?;
        self.clear_diff_state();
        self.workspace.status_message = Some(format!("Loaded {}", path.display()));
        Ok(())
    }

    fn refresh_visible_tables(&mut self) {
        if let Some(path) = self.workspace.left.path.clone() {
            if let Some(selected) = self.workspace.left.selected_table.clone() {
                if let Some(summary) = &self.workspace.left.summary {
                    if let Some(table) = summary.tables.iter().find(|table| table.name == selected)
                    {
                        if let Ok(page) = read_table_page_for_table(
                            &path,
                            table,
                            &self.workspace.left.table_query,
                        ) {
                            self.workspace.left.table_page = Some(page);
                        }
                    }
                }
            }
        }
        if let Some(path) = self.workspace.right.path.clone() {
            if let Some(selected) = self.workspace.right.selected_table.clone() {
                if let Some(summary) = &self.workspace.right.summary {
                    if let Some(table) = summary.tables.iter().find(|table| table.name == selected)
                    {
                        if let Ok(page) = read_table_page_for_table(
                            &path,
                            table,
                            &self.workspace.right.table_query,
                        ) {
                            self.workspace.right.table_page = Some(page);
                        }
                    }
                }
            }
        }
    }

    fn request_diff(&mut self) {
        let Some(left) = self.workspace.left.path.clone() else {
            self.workspace.diff.error =
                Some("Load a left-side database before computing a diff.".to_owned());
            self.workspace.status_message = self.workspace.diff.error.clone();
            return;
        };
        let Some(right) = self.workspace.right.path.clone() else {
            self.workspace.diff.error =
                Some("Load a right-side database before computing a diff.".to_owned());
            self.workspace.status_message = self.workspace.diff.error.clone();
            return;
        };

        let request = DiffRequest {
            left_path: left.clone(),
            right_path: right.clone(),
        };
        let (sender, receiver) = mpsc::channel();

        self.running_diff = Some(RunningDiffTask { request, receiver });
        self.workspace.diff.result = None;
        self.workspace.diff.is_computing = true;
        self.workspace.diff.selected_table = None;
        self.workspace.diff.error = None;
        self.workspace.active_view = WorkspaceView::Diff;
        self.workspace.status_message = Some(format!(
            "Computing diff for {} and {}...",
            left.display(),
            right.display()
        ));

        std::thread::spawn(move || {
            let result = diff_databases(&left, &right).map_err(|error| error.to_string());
            let _ = sender.send(result);
        });
    }

    fn clear_diff_state(&mut self) {
        self.running_diff = None;
        self.workspace.diff.result = None;
        self.workspace.diff.is_computing = false;
        self.workspace.diff.selected_table = None;
        self.workspace.diff.error = None;
    }

    fn poll_running_diff(&mut self, ctx: &egui::Context) {
        let Some(result) = self
            .running_diff
            .as_ref()
            .map(|task| task.receiver.try_recv())
        else {
            return;
        };

        match result {
            Ok(result) => {
                let Some(task) = self.running_diff.take() else {
                    return;
                };
                self.workspace.diff.is_computing = false;
                match result {
                    Ok(diff) => {
                        self.workspace.diff.selected_table = diff
                            .data_diffs
                            .first()
                            .map(|table| table.table_name.clone());
                        self.workspace.diff.result = Some(diff);
                        self.workspace.diff.error = None;
                        self.workspace.active_view = WorkspaceView::Diff;
                        self.workspace.status_message = Some(format!(
                            "Computed database diff for {} and {}.",
                            task.request.left_path.display(),
                            task.request.right_path.display()
                        ));
                    }
                    Err(error) => {
                        self.workspace.diff.result = None;
                        self.workspace.diff.error = Some(error.clone());
                        self.workspace.status_message = Some(format!("Diff failed: {error}"));
                    }
                }
            }
            Err(TryRecvError::Empty) => {
                ctx.request_repaint_after(Duration::from_millis(50));
            }
            Err(TryRecvError::Disconnected) => {
                self.running_diff = None;
                self.workspace.diff.is_computing = false;
                self.workspace.diff.result = None;
                self.workspace.diff.error = Some("Diff worker stopped unexpectedly.".to_owned());
                self.workspace.status_message =
                    Some("Diff failed: worker stopped unexpectedly.".to_owned());
            }
        }
    }

    fn save_snapshot(&mut self) {
        let Some(path) = self.workspace.left.path.clone() else {
            self.workspace.status_message =
                Some("Load a left-side database before saving a snapshot.".to_owned());
            return;
        };
        let snapshot_name = if self.workspace.snapshot_name.trim().is_empty() {
            "Snapshot"
        } else {
            self.workspace.snapshot_name.trim()
        };
        match self.snapshot_store.save_snapshot(&path, snapshot_name) {
            Ok(snapshot) => {
                self.workspace.status_message =
                    Some(format!("Saved snapshot `{}`.", snapshot.name));
                if let Ok(list) = self.snapshot_store.list_snapshots(&path) {
                    self.workspace.left.snapshots = list;
                }
            }
            Err(error) => {
                self.workspace.status_message = Some(format!("Snapshot failed: {error}"));
            }
        }
    }

    fn load_snapshot_as_right(&mut self, snapshot_id: &str) {
        match self.snapshot_store.load_snapshot_path(snapshot_id) {
            Ok(path) => {
                if let Err(error) = self.load_right(path.clone()) {
                    self.workspace.status_message =
                        Some(format!("Failed to load snapshot: {error}"));
                } else {
                    self.request_diff();
                    self.workspace.status_message = Some(format!(
                        "Loaded snapshot {} as the right-hand comparison target.",
                        path.display()
                    ));
                }
            }
            Err(error) => {
                self.workspace.status_message = Some(format!("Failed to find snapshot: {error}"));
            }
        }
    }

    fn render_toolbar(&mut self, ui: &mut egui::Ui) {
        if ui.button("Open Left").clicked() {
            if let Some(path) = ui::dialogs::open_database_dialog() {
                if let Err(error) = self.load_left(path) {
                    self.workspace.status_message =
                        Some(format!("Failed to load database: {error}"));
                }
            }
        }
        if ui.button("Open Right").clicked() {
            if let Some(path) = ui::dialogs::open_database_dialog() {
                if let Err(error) = self.load_right(path) {
                    self.workspace.status_message =
                        Some(format!("Failed to load database: {error}"));
                }
            }
        }
        if ui
            .add_enabled(!self.workspace.diff.is_computing, egui::Button::new("Diff"))
            .clicked()
        {
            self.request_diff();
        }
        if self.workspace.diff.is_computing {
            ui.add(egui::Spinner::new());
        }
        ui.separator();
        ui.label("Snapshot name:");
        ui.text_edit_singleline(&mut self.workspace.snapshot_name);
        if ui.button("Save Snapshot").clicked() {
            self.save_snapshot();
        }
        ui.separator();
        ui::workspace::render_view_switcher(ui, &mut self.workspace.active_view);
    }
}

impl eframe::App for PatchworksApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_running_diff(ctx);

        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| self.render_toolbar(ui));
        });

        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            if let Some(status) = &self.workspace.status_message {
                ui.label(status);
            }
        });

        egui::SidePanel::left("left-panel")
            .resizable(true)
            .show(ctx, |ui| {
                if ui::file_panel::render_file_panel(ui, "Left", &mut self.workspace.left) {
                    self.refresh_visible_tables();
                }
            });

        egui::SidePanel::right("right-panel")
            .resizable(true)
            .show(ctx, |ui| {
                if ui::file_panel::render_file_panel(ui, "Right", &mut self.workspace.right) {
                    self.refresh_visible_tables();
                }
            });

        egui::CentralPanel::default().show(ctx, |ui| match self.workspace.active_view {
            WorkspaceView::Table => {
                ui.columns(2, |columns| {
                    if ui::table_view::render_table_view(&mut columns[0], &mut self.workspace.left)
                    {
                        self.refresh_visible_tables();
                    }
                    if ui::table_view::render_table_view(&mut columns[1], &mut self.workspace.right)
                    {
                        self.refresh_visible_tables();
                    }
                });
            }
            WorkspaceView::Diff => {
                ui::diff_view::render_diff_view(ui, &mut self.workspace.diff);
            }
            WorkspaceView::SchemaDiff => {
                if let Some(DatabaseDiff { schema, .. }) = &self.workspace.diff.result {
                    ui::schema_diff::render_schema_diff(ui, schema);
                } else {
                    ui.label("Compute a diff to view schema changes.");
                }
            }
            WorkspaceView::Snapshots => {
                if let Some(snapshot_id) =
                    ui::snapshot_panel::render_snapshot_panel(ui, &self.workspace.left.snapshots)
                {
                    self.load_snapshot_as_right(&snapshot_id);
                }
            }
            WorkspaceView::SqlExport => {
                if let Some(diff) = &self.workspace.diff.result {
                    ui::sql_export::render_sql_export(
                        ui,
                        &diff.sql_export,
                        &mut self.workspace.status_message,
                    );
                } else {
                    ui.label("Compute a diff to preview SQL export.");
                }
            }
        });
    }
}

fn load_into_pane(pane: &mut DatabasePaneState, store: &SnapshotStore, path: &Path) -> Result<()> {
    let summary = inspect_database(path)?;
    let selected_table = summary.tables.first().map(|table| table.name.clone());
    let table_page = if let Some(table_name) = &selected_table {
        let table = summary
            .tables
            .iter()
            .find(|table| table.name == *table_name)
            .ok_or_else(|| crate::error::PatchworksError::MissingTable {
                table: table_name.clone(),
                path: path.to_path_buf(),
            })?;
        Some(read_table_page_for_table(path, table, &pane.table_query)?)
    } else {
        None
    };

    pane.path = Some(path.to_path_buf());
    pane.summary = Some(summary);
    pane.selected_table = selected_table;
    pane.table_page = table_page;
    pane.snapshots = store.list_snapshots(path).unwrap_or_default();
    pane.error = None;
    Ok(())
}

#[derive(Debug)]
struct RunningDiffTask {
    request: DiffRequest,
    receiver: Receiver<DiffTaskResult>,
}

#[derive(Clone, Debug)]
struct DiffRequest {
    left_path: PathBuf,
    right_path: PathBuf,
}

type DiffTaskResult = std::result::Result<DatabaseDiff, String>;

#[cfg(test)]
mod tests {
    use super::*;

    use rusqlite::Connection;
    use tempfile::TempDir;

    use crate::db::types::{DatabaseSummary, SchemaDiff};

    #[test]
    fn poll_running_diff_applies_completed_result() -> Result<()> {
        let fixture = FixtureDbs::new()?;
        let mut app = PatchworksApp::new(StartupOptions::default())?;
        let (sender, receiver) = mpsc::channel();

        app.running_diff = Some(RunningDiffTask {
            request: DiffRequest {
                left_path: fixture.left.clone(),
                right_path: fixture.right.clone(),
            },
            receiver,
        });
        app.workspace.diff.is_computing = true;
        assert!(sender.send(Ok(sample_diff(&fixture))).is_ok());

        app.poll_running_diff(&egui::Context::default());

        assert!(!app.workspace.diff.is_computing);
        assert!(app.running_diff.is_none());
        assert_eq!(app.workspace.active_view, WorkspaceView::Diff);
        assert_eq!(
            app.workspace.diff.selected_table.as_deref(),
            Some("widgets")
        );
        assert!(app.workspace.diff.result.is_some());
        assert!(app.workspace.diff.error.is_none());
        Ok(())
    }

    #[test]
    fn loading_new_database_cancels_inflight_diff() -> Result<()> {
        let fixture = FixtureDbs::new()?;
        let replacement = fixture.create_db(
            "replacement.sqlite",
            &["CREATE TABLE alt (id INTEGER PRIMARY KEY);"],
        )?;
        let mut app = PatchworksApp::new(StartupOptions::default())?;
        let (sender, receiver) = mpsc::channel();

        app.load_left(fixture.left.clone())?;
        app.workspace.diff.result = Some(sample_diff(&fixture));
        app.workspace.diff.is_computing = true;
        app.running_diff = Some(RunningDiffTask {
            request: DiffRequest {
                left_path: fixture.left.clone(),
                right_path: fixture.right.clone(),
            },
            receiver,
        });

        app.load_right(replacement)?;

        assert!(app.running_diff.is_none());
        assert!(!app.workspace.diff.is_computing);
        assert!(app.workspace.diff.result.is_none());
        assert!(app.workspace.diff.error.is_none());
        assert!(sender.send(Ok(sample_diff(&fixture))).is_err());
        Ok(())
    }

    struct FixtureDbs {
        _tempdir: TempDir,
        left: PathBuf,
        right: PathBuf,
    }

    impl FixtureDbs {
        fn new() -> Result<Self> {
            let tempdir = tempfile::tempdir()?;
            let left = Self::create_db_at(
                tempdir.path().join("left.sqlite"),
                &[
                    "CREATE TABLE widgets (id INTEGER PRIMARY KEY, name TEXT NOT NULL);",
                    "INSERT INTO widgets (id, name) VALUES (1, 'left');",
                ],
            )?;
            let right = Self::create_db_at(
                tempdir.path().join("right.sqlite"),
                &[
                    "CREATE TABLE widgets (id INTEGER PRIMARY KEY, name TEXT NOT NULL);",
                    "INSERT INTO widgets (id, name) VALUES (1, 'right');",
                ],
            )?;

            Ok(Self {
                _tempdir: tempdir,
                left,
                right,
            })
        }

        fn create_db(&self, name: &str, statements: &[&str]) -> Result<PathBuf> {
            Self::create_db_at(self._tempdir.path().join(name), statements)
        }

        fn create_db_at(path: PathBuf, statements: &[&str]) -> Result<PathBuf> {
            let connection = Connection::open(&path)?;
            for statement in statements {
                connection.execute_batch(statement)?;
            }
            Ok(path)
        }
    }

    fn sample_diff(fixture: &FixtureDbs) -> DatabaseDiff {
        DatabaseDiff {
            left: DatabaseSummary {
                path: fixture.left.display().to_string(),
                tables: Vec::new(),
                views: Vec::new(),
                indexes: Vec::new(),
                triggers: Vec::new(),
            },
            right: DatabaseSummary {
                path: fixture.right.display().to_string(),
                tables: Vec::new(),
                views: Vec::new(),
                indexes: Vec::new(),
                triggers: Vec::new(),
            },
            schema: SchemaDiff {
                added_tables: Vec::new(),
                removed_tables: Vec::new(),
                modified_tables: Vec::new(),
                unchanged_tables: vec!["widgets".to_owned()],
                added_indexes: Vec::new(),
                removed_indexes: Vec::new(),
                modified_indexes: Vec::new(),
                added_triggers: Vec::new(),
                removed_triggers: Vec::new(),
                modified_triggers: Vec::new(),
            },
            data_diffs: vec![crate::db::types::TableDataDiff {
                table_name: "widgets".to_owned(),
                columns: vec!["id".to_owned(), "name".to_owned()],
                added_rows: Vec::new(),
                removed_rows: Vec::new(),
                removed_row_keys: Vec::new(),
                modified_rows: Vec::new(),
                stats: Default::default(),
                warnings: Vec::new(),
            }],
            sql_export: String::new(),
        }
    }
}
