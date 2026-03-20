//! Main egui application for Patchworks.

use std::path::{Path, PathBuf};

use eframe::egui;

use crate::db::differ::{diff_databases, DatabaseDiff};
use crate::db::inspector::{inspect_database, read_table_page};
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
}

impl PatchworksApp {
    /// Creates the application and eagerly loads CLI-provided files.
    pub fn new(startup: StartupOptions) -> Result<Self> {
        let mut app = Self {
            workspace: WorkspaceState::default(),
            snapshot_store: SnapshotStore::new_default()?,
        };
        if let Some(path) = startup.left {
            app.load_left(path)?;
        }
        if let Some(path) = startup.right {
            app.load_right(path)?;
            app.compute_diff()?;
        }
        Ok(app)
    }

    fn load_left(&mut self, path: PathBuf) -> Result<()> {
        load_into_pane(&mut self.workspace.left, &self.snapshot_store, &path)?;
        self.workspace.status_message = Some(format!("Loaded {}", path.display()));
        Ok(())
    }

    fn load_right(&mut self, path: PathBuf) -> Result<()> {
        load_into_pane(&mut self.workspace.right, &self.snapshot_store, &path)?;
        self.workspace.status_message = Some(format!("Loaded {}", path.display()));
        Ok(())
    }

    fn refresh_visible_tables(&mut self) {
        if let Some(path) = self.workspace.left.path.clone() {
            if let Some(selected) = self.workspace.left.selected_table.clone() {
                if let Ok(page) =
                    read_table_page(&path, &selected, &self.workspace.left.table_query)
                {
                    self.workspace.left.table_page = Some(page);
                }
            }
        }
        if let Some(path) = self.workspace.right.path.clone() {
            if let Some(selected) = self.workspace.right.selected_table.clone() {
                if let Ok(page) =
                    read_table_page(&path, &selected, &self.workspace.right.table_query)
                {
                    self.workspace.right.table_page = Some(page);
                }
            }
        }
    }

    fn compute_diff(&mut self) -> Result<()> {
        let Some(left) = self.workspace.left.path.clone() else {
            return Ok(());
        };
        let Some(right) = self.workspace.right.path.clone() else {
            return Ok(());
        };
        let diff = diff_databases(&left, &right)?;
        self.workspace.diff.selected_table = diff
            .data_diffs
            .first()
            .map(|table| table.table_name.clone());
        self.workspace.diff.result = Some(diff);
        self.workspace.active_view = WorkspaceView::Diff;
        self.workspace.status_message = Some("Computed database diff.".to_owned());
        Ok(())
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
                if let Err(error) = self
                    .load_right(path.clone())
                    .and_then(|()| self.compute_diff())
                {
                    self.workspace.status_message =
                        Some(format!("Failed to load snapshot: {error}"));
                } else {
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
        if ui.button("Diff").clicked() {
            if let Err(error) = self.compute_diff() {
                self.workspace.status_message = Some(format!("Diff failed: {error}"));
            }
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
        Some(read_table_page(path, table_name, &pane.table_query)?)
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
