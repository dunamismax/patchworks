//! Database file summary and table browser UI.

use egui::{Color32, RichText, Ui};

use crate::state::workspace::DatabasePaneState;

/// Renders a database summary and table list for one side of the workspace.
pub fn render_file_panel(ui: &mut Ui, title: &str, pane: &mut DatabasePaneState) -> bool {
    let mut selected_changed = false;
    ui.heading(title);

    if let Some(path) = &pane.path {
        ui.label(path.display().to_string());
    } else {
        ui.label(RichText::new("No database loaded").italics());
    }

    if let Some(error) = &pane.error {
        ui.colored_label(Color32::RED, error);
    }

    if pane.is_loading {
        ui.horizontal(|ui| {
            ui.add(egui::Spinner::new());
            ui.label("Loading database...");
        });
    }

    if let Some(summary) = &pane.summary {
        ui.separator();
        ui.label(format!(
            "{} tables, {} views",
            summary.tables.len(),
            summary.views.len()
        ));
        ui.separator();
        ui.label(RichText::new("Tables").strong());
        for table in &summary.tables {
            let is_selected = pane.selected_table.as_deref() == Some(table.name.as_str());
            if ui
                .selectable_label(is_selected, format!("{} ({})", table.name, table.row_count))
                .clicked()
            {
                pane.selected_table = Some(table.name.clone());
                pane.table_query.page = 0;
                selected_changed = true;
            }
        }
        if !summary.views.is_empty() {
            ui.separator();
            ui.label(RichText::new("Views").strong());
            for view in &summary.views {
                ui.label(&view.name);
            }
        }
    }

    selected_changed
}
