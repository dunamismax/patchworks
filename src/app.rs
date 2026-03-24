//! Main egui application for Patchworks.

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::Duration;

use eframe::egui;

use crate::db::differ::{
    diff_databases_with_progress, DatabaseDiff, DiffProgress, DiffProgressPhase,
};
use crate::db::inspector::{inspect_database, read_table_page_for_table};
use crate::db::snapshot::SnapshotStore;
use crate::db::types::{Snapshot, TableInfo, TablePage};
use crate::error::{PatchworksError, Result};
use crate::state::recent;
use crate::state::workspace::{
    DatabasePaneState, ProgressState, ThemePreference, WorkspaceState, WorkspaceView,
};
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
        app.workspace.recent_files = recent::load_recent_files();
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
        recent::push_recent_file(&path);
        self.workspace.recent_files = recent::load_recent_files();
        self.start_pane_load(PaneSide::Left, path.clone());
        self.clear_diff_state();
        self.workspace.status_message = Some(format!("Loading {}...", path.display()));
        Ok(())
    }

    fn load_right(&mut self, path: PathBuf) -> Result<()> {
        recent::push_recent_file(&path);
        self.workspace.recent_files = recent::load_recent_files();
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
            pane.progress = Some(ProgressState::new(
                "Inspecting database schema and table counts...",
                0,
                Some(3),
            ));
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
            let result = (|| -> std::result::Result<PaneLoadPayload, String> {
                let _ = sender.send(PaneLoadTaskMessage::Progress(ProgressState::new(
                    "Inspecting database schema and table counts...",
                    0,
                    Some(3),
                )));
                let summary = inspect_database(&path).map_err(|error| error.to_string())?;
                let selected_table = summary.tables.first().map(|table| table.name.clone());
                let table_page = if let Some(table_name) = &selected_table {
                    let table = summary
                        .tables
                        .iter()
                        .find(|table| table.name == *table_name)
                        .cloned()
                        .ok_or_else(|| {
                            PatchworksError::MissingTable {
                                table: table_name.clone(),
                                path: path.clone(),
                            }
                            .to_string()
                        })?;
                    let _ = sender.send(PaneLoadTaskMessage::Progress(ProgressState::new(
                        format!("Loading initial table page for `{table_name}`..."),
                        1,
                        Some(3),
                    )));
                    Some(
                        read_table_page_for_table(&path, &table, &query)
                            .map_err(|error| error.to_string())?,
                    )
                } else {
                    None
                };

                let _ = sender.send(PaneLoadTaskMessage::Progress(ProgressState::new(
                    "Loading snapshots...",
                    2,
                    Some(3),
                )));
                let snapshots = store.list_snapshots(&path).unwrap_or_default();

                Ok(PaneLoadPayload {
                    summary,
                    selected_table,
                    table_page,
                    snapshots,
                })
            })();
            let _ = sender.send(PaneLoadTaskMessage::Complete(Box::new(result)));
        });
    }

    fn request_table_refresh(&mut self, side: PaneSide) {
        let Some(path) = self.pane(side).path.clone() else {
            return;
        };
        let Some(selected_table) = self.pane(side).selected_table.clone() else {
            let pane = self.pane_mut(side);
            pane.is_loading_table = false;
            pane.progress = None;
            pane.table_page = None;
            self.set_running_table_load(side, None);
            return;
        };
        let query = self.pane(side).table_query.clone();
        let Some(table) = self.lookup_selected_table(side, &selected_table) else {
            let pane = self.pane_mut(side);
            pane.is_loading_table = false;
            pane.progress = None;
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
            pane.progress = Some(ProgressState::new(
                format!(
                    "Loading table `{selected_table}` page {}...",
                    query.page + 1
                ),
                0,
                Some(1),
            ));
            pane.table_page = None;
            pane.error = None;
        }

        let (sender, receiver) = mpsc::channel();
        self.set_running_table_load(
            side,
            Some(RunningTableLoadTask {
                request: TableLoadRequest {
                    table_name: selected_table.clone(),
                },
                receiver,
            }),
        );

        std::thread::spawn(move || {
            let _ = sender.send(TableLoadTaskMessage::Progress(ProgressState::new(
                format!(
                    "Loading table `{selected_table}` page {}...",
                    query.page + 1
                ),
                0,
                Some(1),
            )));
            let result =
                read_table_page_for_table(&path, &table, &query).map_err(|error| error.to_string());
            let _ = sender.send(TableLoadTaskMessage::Complete(result));
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
        self.workspace.diff.progress = Some(ProgressState::new(
            "Inspecting left database schema and table counts...",
            0,
            None,
        ));
        self.workspace.diff.selected_table = None;
        self.workspace.diff.error = None;
        self.workspace.active_view = WorkspaceView::Diff;
        self.workspace.status_message = Some(format!(
            "Computing diff for {} and {}...",
            left.display(),
            right.display()
        ));

        std::thread::spawn(move || {
            let result = diff_databases_with_progress(&left, &right, |progress| {
                let _ = sender.send(DiffTaskMessage::Progress(progress));
            })
            .map_err(|error| error.to_string());
            let _ = sender.send(DiffTaskMessage::Complete(Box::new(result)));
        });
    }

    fn clear_diff_state(&mut self) {
        self.running_diff = None;
        self.workspace.diff.result = None;
        self.workspace.diff.is_computing = false;
        self.workspace.diff.progress = None;
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
        loop {
            let message = match self.running_pane_load(side).as_ref() {
                Some(task) => task.receiver.try_recv(),
                None => return,
            };
            match message {
                Ok(PaneLoadTaskMessage::Progress(progress)) => {
                    let pane = self.pane_mut(side);
                    pane.progress = Some(progress.clone());
                    self.workspace.status_message = Some(progress.label);
                }
                Ok(PaneLoadTaskMessage::Complete(result)) => {
                    let Some(task) = self.running_pane_load_mut(side).take() else {
                        return;
                    };
                    let pane = self.pane_mut(side);
                    pane.is_loading = false;
                    pane.is_loading_table = false;
                    pane.progress = None;
                    match *result {
                        Ok(payload) => {
                            pane.path = Some(task.request.path.clone());
                            pane.summary = Some(payload.summary);
                            pane.selected_table = payload.selected_table;
                            pane.table_page = payload.table_page;
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
                    return;
                }
                Err(TryRecvError::Empty) => {
                    ctx.request_repaint_after(Duration::from_millis(50));
                    return;
                }
                Err(TryRecvError::Disconnected) => {
                    self.set_running_pane_load(side, None);
                    let pane = self.pane_mut(side);
                    pane.is_loading = false;
                    pane.is_loading_table = false;
                    pane.progress = None;
                    pane.summary = None;
                    pane.selected_table = None;
                    pane.table_page = None;
                    pane.snapshots.clear();
                    pane.error = Some("Database loader stopped unexpectedly.".to_owned());
                    self.workspace.status_message =
                        Some("Failed to load database: worker stopped unexpectedly.".to_owned());
                    return;
                }
            }
        }
    }

    fn poll_running_table_load(&mut self, side: PaneSide, ctx: &egui::Context) {
        loop {
            let message = match self.running_table_load(side).as_ref() {
                Some(task) => task.receiver.try_recv(),
                None => return,
            };
            match message {
                Ok(TableLoadTaskMessage::Progress(progress)) => {
                    let pane = self.pane_mut(side);
                    pane.progress = Some(progress.clone());
                    self.workspace.status_message = Some(progress.label);
                }
                Ok(TableLoadTaskMessage::Complete(result)) => {
                    let Some(task) = self.running_table_load_mut(side).take() else {
                        return;
                    };
                    let pane = self.pane_mut(side);
                    pane.is_loading_table = false;
                    pane.progress = None;
                    match result {
                        Ok(page) => {
                            let page_number = page.page + 1;
                            pane.selected_table = Some(task.request.table_name.clone());
                            pane.table_page = Some(page);
                            pane.error = None;
                            self.workspace.status_message = Some(format!(
                                "Loaded table `{}` page {}.",
                                task.request.table_name, page_number
                            ));
                        }
                        Err(error) => {
                            pane.table_page = None;
                            pane.error = Some(error.clone());
                            self.workspace.status_message =
                                Some(format!("Failed to load table page: {error}"));
                        }
                    }
                    return;
                }
                Err(TryRecvError::Empty) => {
                    ctx.request_repaint_after(Duration::from_millis(50));
                    return;
                }
                Err(TryRecvError::Disconnected) => {
                    self.set_running_table_load(side, None);
                    let pane = self.pane_mut(side);
                    pane.is_loading_table = false;
                    pane.progress = None;
                    pane.table_page = None;
                    pane.error = Some("Table loader stopped unexpectedly.".to_owned());
                    self.workspace.status_message =
                        Some("Failed to load table page: worker stopped unexpectedly.".to_owned());
                    return;
                }
            }
        }
    }

    fn poll_running_diff(&mut self, ctx: &egui::Context) {
        loop {
            let message = match self.running_diff.as_ref() {
                Some(task) => task.receiver.try_recv(),
                None => return,
            };
            match message {
                Ok(DiffTaskMessage::Progress(progress)) => {
                    let progress = map_diff_progress(progress);
                    self.workspace.diff.progress = Some(progress.clone());
                    self.workspace.status_message = Some(progress.label);
                }
                Ok(DiffTaskMessage::Complete(result)) => {
                    let Some(task) = self.running_diff.take() else {
                        return;
                    };
                    self.workspace.diff.is_computing = false;
                    self.workspace.diff.progress = None;
                    match *result {
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
                    return;
                }
                Err(TryRecvError::Empty) => {
                    ctx.request_repaint_after(Duration::from_millis(50));
                    return;
                }
                Err(TryRecvError::Disconnected) => {
                    self.running_diff = None;
                    self.workspace.diff.is_computing = false;
                    self.workspace.diff.progress = None;
                    self.workspace.diff.result = None;
                    self.workspace.diff.error =
                        Some("Diff worker stopped unexpectedly.".to_owned());
                    self.workspace.status_message =
                        Some("Diff failed: worker stopped unexpectedly.".to_owned());
                    return;
                }
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

        // Recent files menu
        if !self.workspace.recent_files.is_empty() {
            ui.menu_button("Recent", |ui| {
                let mut load_left = None;
                let mut load_right = None;
                for path in &self.workspace.recent_files {
                    let label = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("(unknown)")
                        .to_owned();
                    ui.horizontal(|ui| {
                        if ui.small_button("L").on_hover_text("Open as Left").clicked() {
                            load_left = Some(path.clone());
                        }
                        if ui
                            .small_button("R")
                            .on_hover_text("Open as Right")
                            .clicked()
                        {
                            load_right = Some(path.clone());
                        }
                        ui.label(&label).on_hover_text(path.display().to_string());
                    });
                }
                if let Some(path) = load_left {
                    ui.close();
                    if let Err(error) = self.load_left(path) {
                        self.workspace.status_message =
                            Some(format!("Failed to load database: {error}"));
                    }
                }
                if let Some(path) = load_right {
                    ui.close();
                    if let Err(error) = self.load_right(path) {
                        self.workspace.status_message =
                            Some(format!("Failed to load database: {error}"));
                    }
                }
            });
        }

        ui.separator();
        if ui
            .add_enabled(!self.workspace.diff.is_computing, egui::Button::new("Diff"))
            .on_hover_text("Compare loaded databases (⌘D)")
            .clicked()
        {
            self.request_diff();
        }
        if self.workspace.diff.is_computing {
            ui.add(egui::Spinner::new());
        }
        ui.separator();
        ui.label("Snapshot:");
        ui.text_edit_singleline(&mut self.workspace.snapshot_name);
        if ui
            .button("Save")
            .on_hover_text("Save snapshot of left database")
            .clicked()
        {
            self.save_snapshot();
        }
        ui.separator();
        ui::workspace::render_view_switcher(ui, &mut self.workspace.active_view);
        ui.separator();
        // Theme switcher
        egui::ComboBox::from_id_salt("theme-combo")
            .selected_text(match self.workspace.theme {
                ThemePreference::System => "System",
                ThemePreference::Dark => "Dark",
                ThemePreference::Light => "Light",
            })
            .width(60.0)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut self.workspace.theme, ThemePreference::System, "System");
                ui.selectable_value(&mut self.workspace.theme, ThemePreference::Dark, "Dark");
                ui.selectable_value(&mut self.workspace.theme, ThemePreference::Light, "Light");
            });
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
        // Apply theme preference
        match self.workspace.theme {
            ThemePreference::Dark => ctx.set_visuals(egui::Visuals::dark()),
            ThemePreference::Light => ctx.set_visuals(egui::Visuals::light()),
            ThemePreference::System => {
                // egui defaults to dark; respect platform hint when available
            }
        }

        // Keyboard shortcuts
        let modifiers = egui::Modifiers::COMMAND;
        if ctx.input(|i| i.key_pressed(egui::Key::D) && i.modifiers.matches_exact(modifiers)) {
            self.request_diff();
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Num1) && i.modifiers.matches_exact(modifiers)) {
            self.workspace.active_view = WorkspaceView::Table;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Num2) && i.modifiers.matches_exact(modifiers)) {
            self.workspace.active_view = WorkspaceView::SchemaBrowser;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Num3) && i.modifiers.matches_exact(modifiers)) {
            self.workspace.active_view = WorkspaceView::Diff;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Num4) && i.modifiers.matches_exact(modifiers)) {
            self.workspace.active_view = WorkspaceView::SchemaDiff;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Num5) && i.modifiers.matches_exact(modifiers)) {
            self.workspace.active_view = WorkspaceView::Snapshots;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Num6) && i.modifiers.matches_exact(modifiers)) {
            self.workspace.active_view = WorkspaceView::SqlExport;
        }

        self.poll_background_work(ctx);

        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| self.render_toolbar(ui));
        });

        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if let Some(status) = &self.workspace.status_message {
                    ui.label(status);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(egui::RichText::new("⌘1-6: views  ⌘D: diff").small().weak());
                });
            });
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
            WorkspaceView::SchemaBrowser => {
                let left_summary = self.workspace.left.summary.as_ref();
                let right_summary = self.workspace.right.summary.as_ref();
                ui::schema_browser::render_schema_browser(ui, left_summary, right_summary);
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
    receiver: Receiver<PaneLoadTaskMessage>,
}

#[derive(Clone, Debug)]
struct PaneLoadRequest {
    path: PathBuf,
}

#[derive(Debug)]
struct PaneLoadPayload {
    summary: crate::db::types::DatabaseSummary,
    selected_table: Option<String>,
    table_page: Option<TablePage>,
    snapshots: Vec<Snapshot>,
}

enum PaneLoadTaskMessage {
    Progress(ProgressState),
    Complete(Box<PaneLoadTaskResult>),
}

type PaneLoadTaskResult = std::result::Result<PaneLoadPayload, String>;

#[derive(Debug)]
struct RunningTableLoadTask {
    request: TableLoadRequest,
    receiver: Receiver<TableLoadTaskMessage>,
}

#[derive(Clone, Debug)]
struct TableLoadRequest {
    table_name: String,
}

enum TableLoadTaskMessage {
    Progress(ProgressState),
    Complete(TableLoadTaskResult),
}

type TableLoadTaskResult = std::result::Result<TablePage, String>;

#[derive(Debug)]
struct RunningDiffTask {
    request: DiffRequest,
    receiver: Receiver<DiffTaskMessage>,
}

#[derive(Clone, Debug)]
struct DiffRequest {
    left_path: PathBuf,
    right_path: PathBuf,
}

enum DiffTaskMessage {
    Progress(DiffProgress),
    Complete(Box<DiffTaskResult>),
}

type DiffTaskResult = std::result::Result<DatabaseDiff, String>;

fn map_diff_progress(progress: DiffProgress) -> ProgressState {
    let label = match progress.phase {
        DiffProgressPhase::InspectingLeft => {
            "Inspecting left database schema and table counts...".to_owned()
        }
        DiffProgressPhase::InspectingRight => {
            "Inspecting right database schema and table counts...".to_owned()
        }
        DiffProgressPhase::DiffingSchema => "Comparing schemas...".to_owned(),
        DiffProgressPhase::DiffingTable {
            table_name,
            table_index,
            total_tables,
        } => format!(
            "Diffing table `{table_name}` ({}/{})...",
            table_index + 1,
            total_tables
        ),
        DiffProgressPhase::GeneratingSqlExport => "Generating SQL export...".to_owned(),
    };

    ProgressState::new(label, progress.completed_steps, progress.total_steps)
}

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
        assert_eq!(
            app.workspace.left.progress,
            Some(ProgressState::new(
                "Inspecting database schema and table counts...",
                0,
                Some(3),
            ))
        );
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
        assert!(app.workspace.left.progress.is_none());
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
        assert_eq!(
            app.workspace.left.progress,
            Some(ProgressState::new(
                "Loading table `widgets` page 1...",
                0,
                Some(1),
            ))
        );
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
        assert!(app.workspace.left.progress.is_none());
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

        assert!(old_sender
            .send(TableLoadTaskMessage::Complete(Ok(old_page)))
            .is_err());
        assert!(new_sender
            .send(TableLoadTaskMessage::Complete(Ok(new_page)))
            .is_ok());

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
    fn poll_running_diff_applies_progress_updates() -> Result<()> {
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

        assert!(sender
            .send(DiffTaskMessage::Progress(DiffProgress {
                phase: DiffProgressPhase::DiffingTable {
                    table_name: "widgets".to_owned(),
                    table_index: 0,
                    total_tables: 1,
                },
                completed_steps: 3,
                total_steps: Some(5),
            }))
            .is_ok());

        app.poll_running_diff(&egui::Context::default());

        assert!(app.workspace.diff.is_computing);
        assert_eq!(
            app.workspace.diff.progress,
            Some(ProgressState::new(
                "Diffing table `widgets` (1/1)...",
                3,
                Some(5),
            ))
        );
        assert_eq!(
            app.workspace.status_message.as_deref(),
            Some("Diffing table `widgets` (1/1)...")
        );
        assert!(app.workspace.diff.result.is_none());
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
        app.workspace.diff.progress = Some(ProgressState::new("Comparing schemas...", 2, Some(5)));
        assert!(sender
            .send(DiffTaskMessage::Complete(Box::new(Ok(sample_diff(
                &fixture
            )))))
            .is_ok());

        app.poll_running_diff(&egui::Context::default());

        assert!(!app.workspace.diff.is_computing);
        assert!(app.running_diff.is_none());
        assert!(app.workspace.diff.progress.is_none());
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
    fn loading_new_database_drops_inflight_diff_receiver() -> Result<()> {
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
        app.workspace.diff.progress = Some(ProgressState::new(
            "Diffing table `widgets` (1/1)...",
            3,
            Some(5),
        ));
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
        assert!(app.workspace.diff.progress.is_none());
        assert!(app.workspace.diff.error.is_none());
        assert!(sender
            .send(DiffTaskMessage::Complete(Box::new(Ok(sample_diff(
                &fixture
            )))))
            .is_err());
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
