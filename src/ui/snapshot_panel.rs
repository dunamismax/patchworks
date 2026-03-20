//! Snapshot list rendering.

use egui::Ui;

use crate::db::types::Snapshot;

/// Renders a snapshot list and returns the selected snapshot ID.
pub fn render_snapshot_panel(ui: &mut Ui, snapshots: &[Snapshot]) -> Option<String> {
    let mut selected = None;
    if snapshots.is_empty() {
        ui.label("No snapshots for this database yet.");
        return None;
    }

    for snapshot in snapshots {
        ui.horizontal(|ui| {
            if ui.button("Compare").clicked() {
                selected = Some(snapshot.id.clone());
            }
            ui.label(format!(
                "{} ({}, {} tables, {} rows)",
                snapshot.name, snapshot.created_at, snapshot.table_count, snapshot.total_rows
            ));
        });
    }

    selected
}
