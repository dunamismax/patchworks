//! Main egui application for Patchworks.

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::Duration;

use eframe::egui;

use crate::db::differ::{diff_databases, DatabaseDiff};
use crate::db::inspector::{
    inspect_database_with_page, read_table_page_for_table, InitialInspection,
};
use crate::db::snapshot::SnapshotStore;
use crate::db::types::{Snapshot, TableInfo, TablePage};
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
    running_left_load: Option<RunningPaneLoadTask>,
    running_right_load: Option<RunningPaneLoadTask>,
    running_left_table_load: Option<RunningTableLoadTask>,
    running_right_table_load: Option<RunningTableLoadTask>,
    running_diff: Option<RunningDiffTask>,
}

impl PatchworksApp {
    /// Creates the application and eagerly loads CLI-provided files.
    pub fn new(startup: StartupOptions) -> Result<Self> {
        let mut app = Self {
            workspace: WorkspaceState::default(),
            snapshot_store: SnapshotStore::new_default()?,
            running_left_load: None,
            running_right_load: None,
            running_left_table_load: None,
            running_right_table_load: None,
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
        self.start_pane_load(PaneSide::Left, path.clone());
        self.clear_diff_state();
        self.workspace.status_message = Some(format!("Loading {}...", path.display()));
        Ok(())
    }

    fn load_right(&mut self, path: PathBuf) -> Result<()> {
        self.start_pane_load(PaneSide::Right, path.clone());
        self.clear_diff_state();
        self.workspace.status_message = Some(format!("Loading {}...", path.display()));
        Ok(())
    }

    fn start_pane_load(&mut self, side: PaneSide, path: PathBuf) {
        self.set_running_table_load(side, None);
        self.set_running_pane_load(side, None);

        let query = self.pane(side).table_query.clone();
        let store = self.snapshot_store.clone();
        let (sender, receiver) = mpsc::channel();

        {
            let pane = self.pane_mut(side);
            pane.path = Some(path.clone());
            pane.is_loading = true;
            pane.is_loading_table = false;
            pane.summary = None;
            pane.selected_table = None;
            pane.table_page = None;
            pane.snapshots.clear();
            pane.error = None;
        }

        self.set_running_pane_load(
            side,
            Some(RunningPaneLoadTask {
                request: PaneLoadRequest { path: path.clone() },
                receiver,
            }),
        );

        std::thread::spawn(move || {
            let result = inspect_database_with_page(&path, &query)
                .map(|inspection| PaneLoadPayload {
                    inspection,
                    snapshots: store.list_snapshots(&path).unwrap_or_default(),
                })
                .map_err(|error| error.to_string());
            let _ = sender.send(result);
        });
    }

    fn request_table_refresh(&mut self, side: PaneSide) {
        let Some(path) = self.pane(side).path.clone() else {
            return;
        };
        let Some(selected_table) = self.pane(side).selected_table.clone() else {
            let pane = self.pane_mut(side);
            pane.is_loading_table = false;
            pane.table_page = None;
            self.set_running_table_load(side, None);
            return;
        };
        let query = self.pane(side).table_query.clone();
        let Some(table) = self.lookup_selected_table(side, &selected_table) else {
            let pane = self.pane_mut(side);
            pane.is_loading_table = false;
            pane.table_page = None;
            pane.error = Some(format!(
                "Selected table `{selected_table}` no longer exists in {}.",
                path.display()
            ));
            self.workspace.status_message = pane.error.clone();
            self.set_running_table_load(side, None);
            return;
        };

        self.set_running_table_load(side, None);
        {
            let pane = self.pane_mut(side);
            pane.is_loading_table = true;
            pane.table_page = None;
            pane.error = None;
        }

        let (sender, receiver) = mpsc::channel();
        self.set_running_table_load(
            side,
            Some(RunningTableLoadTask {
                request: TableLoadRequest {
                    table_name: selected_table,
                },
                receiver,
            }),
        );

        std::thread::spawn(move || {
            let result =
                read_table_page_for_table(&path, &table, &query).map_err(|error| error.to_string());
            let _ = sender.send(result);
        });
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

    fn poll_background_work(&mut self, ctx: &egui::Context) {
        self.poll_running_pane_load(PaneSide::Left, ctx);
        self.poll_running_pane_load(PaneSide::Right, ctx);
        self.poll_running_table_load(PaneSide::Left, ctx);
        self.poll_running_table_load(PaneSide::Right, ctx);
        self.poll_running_diff(ctx);
    }

    fn poll_running_pane_load(&mut self, side: PaneSide, ctx: &egui::Context) {
        let Some(result) = self
            .running_pane_load(side)
            .as_ref()
            .map(|task| task.receiver.try_recv())
        else {
            return;
        };

        match result {
            Ok(result) => {
                let Some(task) = self.running_pane_load_mut(side).take() else {
                    return;
                };
                let pane = self.pane_mut(side);
                pane.is_loading = false;
                pane.is_loading_table = false;
                match result {
                    Ok(payload) => {
                        pane.path = Some(task.request.path.clone());
                        pane.summary = Some(payload.inspection.summary);
                        pane.selected_table = payload.inspection.selected_table;
                        pane.table_page = payload.inspection.table_page;
                        pane.snapshots = payload.snapshots;
                        pane.error = None;
                        self.workspace.status_message =
                            Some(format!("Loaded {}", task.request.path.display()));
                    }
                    Err(error) => {
                        pane.summary = None;
                        pane.selected_table = None;
                        pane.table_page = None;
                        pane.snapshots.clear();
                        pane.error = Some(error.clone());
                        self.workspace.status_message =
                            Some(format!("Failed to load database: {error}"));
                    }
                }
            }
            Err(TryRecvError::Empty) => {
                ctx.request_repaint_after(Duration::from_millis(50));
            }
            Err(TryRecvError::Disconnected) => {
                self.set_running_pane_load(side, None);
                let pane = self.pane_mut(side);
                pane.is_loading = false;
                pane.is_loading_table = false;
                pane.summary = None;
                pane.selected_table = None;
                pane.table_page = None;
                pane.snapshots.clear();
                pane.error = Some("Database loader stopped unexpectedly.".to_owned());
                self.workspace.status_message =
                    Some("Failed to load database: worker stopped unexpectedly.".to_owned());
            }
        }
    }

    fn poll_running_table_load(&mut self, side: PaneSide, ctx: &egui::Context) {
        let Some(result) = self
            .running_table_load(side)
            .as_ref()
            .map(|task| task.receiver.try_recv())
        else {
            return;
        };

        match result {
            Ok(result) => {
                let Some(task) = self.running_table_load_mut(side).take() else {
                    return;
                };
                let pane = self.pane_mut(side);
                pane.is_loading_table = false;
                match result {
                    Ok(page) => {
                        pane.selected_table = Some(task.request.table_name);
                        pane.table_page = Some(page);
                        pane.error = None;
                    }
                    Err(error) => {
                        pane.table_page = None;
                        pane.error = Some(error.clone());
                        self.workspace.status_message =
                            Some(format!("Failed to load table page: {error}"));
                    }
                }
            }
            Err(TryRecvError::Empty) => {
                ctx.request_repaint_after(Duration::from_millis(50));
            }
            Err(TryRecvError::Disconnected) => {
                self.set_running_table_load(side, None);
                let pane = self.pane_mut(side);
                pane.is_loading_table = false;
                pane.table_page = None;
                pane.error = Some("Table loader stopped unexpectedly.".to_owned());
                self.workspace.status_message =
                    Some("Failed to load table page: worker stopped unexpectedly.".to_owned());
            }
        }
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

    fn pane(&self, side: PaneSide) -> &DatabasePaneState {
        match side {
            PaneSide::Left => &self.workspace.left,
            PaneSide::Right => &self.workspace.right,
        }
    }

    fn pane_mut(&mut self, side: PaneSide) -> &mut DatabasePaneState {
        match side {
            PaneSide::Left => &mut self.workspace.left,
            PaneSide::Right => &mut self.workspace.right,
        }
    }

    fn running_pane_load(&self, side: PaneSide) -> &Option<RunningPaneLoadTask> {
        match side {
            PaneSide::Left => &self.running_left_load,
            PaneSide::Right => &self.running_right_load,
        }
    }

    fn running_pane_load_mut(&mut self, side: PaneSide) -> &mut Option<RunningPaneLoadTask> {
        match side {
            PaneSide::Left => &mut self.running_left_load,
            PaneSide::Right => &mut self.running_right_load,
        }
    }

    fn set_running_pane_load(&mut self, side: PaneSide, task: Option<RunningPaneLoadTask>) {
        *self.running_pane_load_mut(side) = task;
    }

    fn running_table_load(&self, side: PaneSide) -> &Option<RunningTableLoadTask> {
        match side {
            PaneSide::Left => &self.running_left_table_load,
            PaneSide::Right => &self.running_right_table_load,
        }
    }

    fn running_table_load_mut(&mut self, side: PaneSide) -> &mut Option<RunningTableLoadTask> {
        match side {
            PaneSide::Left => &mut self.running_left_table_load,
            PaneSide::Right => &mut self.running_right_table_load,
        }
    }

    fn set_running_table_load(&mut self, side: PaneSide, task: Option<RunningTableLoadTask>) {
        *self.running_table_load_mut(side) = task;
    }

    fn lookup_selected_table(&self, side: PaneSide, table_name: &str) -> Option<TableInfo> {
        self.pane(side).summary.as_ref().and_then(|summary| {
            summary
                .tables
                .iter()
                .find(|table| table.name == table_name)
                .cloned()
        })
    }
}

impl eframe::App for PatchworksApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_background_work(ctx);

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
                    self.request_table_refresh(PaneSide::Left);
                }
            });

        egui::SidePanel::right("right-panel")
            .resizable(true)
            .show(ctx, |ui| {
                if ui::file_panel::render_file_panel(ui, "Right", &mut self.workspace.right) {
                    self.request_table_refresh(PaneSide::Right);
                }
            });

        egui::CentralPanel::default().show(ctx, |ui| match self.workspace.active_view {
            WorkspaceView::Table => {
                ui.columns(2, |columns| {
                    if ui::table_view::render_table_view(&mut columns[0], &mut self.workspace.left)
                    {
                        self.request_table_refresh(PaneSide::Left);
                    }
                    if ui::table_view::render_table_view(&mut columns[1], &mut self.workspace.right)
                    {
                        self.request_table_refresh(PaneSide::Right);
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PaneSide {
    Left,
    Right,
}

#[derive(Debug)]
struct RunningPaneLoadTask {
    request: PaneLoadRequest,
    receiver: Receiver<PaneLoadTaskResult>,
}

#[derive(Clone, Debug)]
struct PaneLoadRequest {
    path: PathBuf,
}

#[derive(Debug)]
struct PaneLoadPayload {
    inspection: InitialInspection,
    snapshots: Vec<Snapshot>,
}

type PaneLoadTaskResult = std::result::Result<PaneLoadPayload, String>;

#[derive(Debug)]
struct RunningTableLoadTask {
    request: TableLoadRequest,
    receiver: Receiver<TableLoadTaskResult>,
}

#[derive(Clone, Debug)]
struct TableLoadRequest {
    table_name: String,
}

type TableLoadTaskResult = std::result::Result<TablePage, String>;

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

    use crate::db::inspector::read_table_page;
    use crate::db::types::{DatabaseSummary, SchemaDiff, TableQuery};

    #[test]
    fn left_database_load_runs_in_background_and_applies_result() -> Result<()> {
        let fixture = FixtureDbs::new()?;
        let mut app = PatchworksApp::new(StartupOptions::default())?;

        app.load_left(fixture.left.clone())?;

        assert!(app.workspace.left.is_loading);
        assert!(app.workspace.left.summary.is_none());
        assert!(app.running_left_load.is_some());

        wait_for_pane_load(&mut app, PaneSide::Left);

        assert!(!app.workspace.left.is_loading);
        assert!(app.running_left_load.is_none());
        assert_eq!(app.workspace.left.path.as_ref(), Some(&fixture.left));
        assert_eq!(
            app.workspace.left.selected_table.as_deref(),
            Some("gadgets")
        );
        assert_eq!(
            app.workspace
                .left
                .table_page
                .as_ref()
                .map(|page| page.rows.len()),
            Some(1)
        );
        assert!(app.workspace.left.error.is_none());
        Ok(())
    }

    #[test]
    fn table_refresh_runs_in_background_and_updates_page() -> Result<()> {
        let fixture = FixtureDbs::new()?;
        let mut app = PatchworksApp::new(StartupOptions::default())?;

        app.load_left(fixture.left.clone())?;
        wait_for_pane_load(&mut app, PaneSide::Left);

        app.workspace.left.selected_table = Some("widgets".to_owned());
        app.workspace.left.table_query = TableQuery {
            page: 0,
            page_size: 1,
            sort: None,
        };

        app.request_table_refresh(PaneSide::Left);

        assert!(app.workspace.left.is_loading_table);
        assert!(app.running_left_table_load.is_some());
        assert!(app.workspace.left.table_page.is_none());

        wait_for_table_load(&mut app, PaneSide::Left);

        let page = app
            .workspace
            .left
            .table_page
            .as_ref()
            .expect("table page applied");
        assert_eq!(page.table_name, "widgets");
        assert_eq!(page.rows.len(), 1);
        assert!(!app.workspace.left.is_loading_table);
        Ok(())
    }

    #[test]
    fn replacing_table_refresh_drops_stale_receiver() -> Result<()> {
        let fixture = FixtureDbs::new()?;
        let mut app = PatchworksApp::new(StartupOptions::default())?;
        app.load_left(fixture.left.clone())?;
        wait_for_pane_load(&mut app, PaneSide::Left);

        let old_page = read_table_page(
            &fixture.left,
            "widgets",
            &TableQuery {
                page: 0,
                page_size: 1,
                sort: None,
            },
        )?;
        let new_page = read_table_page(
            &fixture.left,
            "widgets",
            &TableQuery {
                page: 1,
                page_size: 1,
                sort: None,
            },
        )?;

        let (old_sender, old_receiver) = mpsc::channel();
        app.running_left_table_load = Some(RunningTableLoadTask {
            request: TableLoadRequest {
                table_name: "widgets".to_owned(),
            },
            receiver: old_receiver,
        });

        let (new_sender, new_receiver) = mpsc::channel();
        app.set_running_table_load(
            PaneSide::Left,
            Some(RunningTableLoadTask {
                request: TableLoadRequest {
                    table_name: "widgets".to_owned(),
                },
                receiver: new_receiver,
            }),
        );
        app.workspace.left.is_loading_table = true;
        app.workspace.left.table_page = None;

        assert!(old_sender.send(Ok(old_page)).is_err());
        assert!(new_sender.send(Ok(new_page)).is_ok());

        app.poll_running_table_load(PaneSide::Left, &egui::Context::default());

        let page = app
            .workspace
            .left
            .table_page
            .as_ref()
            .expect("newest table page applied");
        assert_eq!(page.page, 1);
        assert!(!app.workspace.left.is_loading_table);
        Ok(())
    }

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

    fn wait_for_pane_load(app: &mut PatchworksApp, side: PaneSide) {
        for _ in 0..200 {
            app.poll_running_pane_load(side, &egui::Context::default());
            if app.running_pane_load(side).is_none() {
                return;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("timed out waiting for pane load to finish");
    }

    fn wait_for_table_load(app: &mut PatchworksApp, side: PaneSide) {
        for _ in 0..200 {
            app.poll_running_table_load(side, &egui::Context::default());
            if app.running_table_load(side).is_none() {
                return;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("timed out waiting for table load to finish");
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
                    "INSERT INTO widgets (id, name) VALUES (1, 'left-a'), (2, 'left-b');",
                    "CREATE TABLE gadgets (id INTEGER PRIMARY KEY, label TEXT NOT NULL);",
                    "INSERT INTO gadgets (id, label) VALUES (1, 'gizmo');",
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
