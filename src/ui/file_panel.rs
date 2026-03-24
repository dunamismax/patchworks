//! Database file summary and table browser UI.

use egui::{Color32, RichText, Ui};

use crate::state::workspace::DatabasePaneState;
use crate::ui::progress;

/// Renders a database summary and table list for one side of the workspace.
pub fn render_file_panel(ui: &mut Ui, title: &str, pane: &mut DatabasePaneState) -> bool {
    let mut selected_changed = false;
    ui.heading(title);

    if let Some(path) = &pane.path {
        let display = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(&path.display().to_string())
            .to_owned();
        ui.label(&display).on_hover_text(path.display().to_string());
    } else {
        ui.label(RichText::new("No database loaded").italics());
    }

    if let Some(error) = &pane.error {
        ui.colored_label(Color32::RED, error);
    }

    if let Some(progress_state) = &pane.progress {
        progress::render_progress(ui, progress_state);
    }

    if let Some(summary) = &pane.summary {
        ui.separator();
        let stats = format!(
            "{} tables, {} views, {} indexes, {} triggers",
            summary.tables.len(),
            summary.views.len(),
            summary.indexes.len(),
            summary.triggers.len(),
        );
        ui.label(stats);
        ui.separator();

        // Table filter
        ui.horizontal(|ui| {
            ui.label("🔍");
            ui.text_edit_singleline(&mut pane.table_filter)
                .on_hover_text("Filter tables by name");
        });

        ui.label(RichText::new("Tables").strong());
        let filter = pane.table_filter.to_lowercase();
        for table in &summary.tables {
            if !filter.is_empty() && !table.name.to_lowercase().contains(&filter) {
                continue;
            }
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
