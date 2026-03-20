//! Shared workspace chrome helpers.

use egui::Ui;

use crate::state::workspace::WorkspaceView;

/// Renders central view toggles.
pub fn render_view_switcher(ui: &mut Ui, active_view: &mut WorkspaceView) {
    ui.selectable_value(active_view, WorkspaceView::Table, "Table");
    ui.selectable_value(active_view, WorkspaceView::Diff, "Diff");
    ui.selectable_value(active_view, WorkspaceView::SchemaDiff, "Schema");
    ui.selectable_value(active_view, WorkspaceView::Snapshots, "Snapshots");
    ui.selectable_value(active_view, WorkspaceView::SqlExport, "SQL Export");
}
