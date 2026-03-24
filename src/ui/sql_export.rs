//! SQL export preview UI.

use std::fs;

use egui::{TextEdit, Ui};

use crate::ui::dialogs::save_sql_dialog;

/// Renders the SQL export preview and save controls.
pub fn render_sql_export(ui: &mut Ui, sql: &str, status: &mut Option<String>) {
    let line_count = sql.lines().count();
    let stmt_count = sql
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            trimmed.ends_with(';') && !trimmed.starts_with("--")
        })
        .count();

    ui.horizontal(|ui| {
        if ui.button("📋 Copy to clipboard").clicked() {
            ui.ctx().copy_text(sql.to_owned());
            *status = Some("Copied SQL export to clipboard.".to_owned());
        }
        if ui.button("💾 Save to file").clicked() {
            if let Some(path) = save_sql_dialog() {
                match fs::write(&path, sql) {
                    Ok(()) => {
                        *status = Some(format!("Saved SQL export to {}", path.display()));
                    }
                    Err(error) => {
                        *status = Some(format!("Failed to save SQL export: {error}"));
                    }
                }
            }
        }
        ui.separator();
        ui.label(format!("{line_count} lines, {stmt_count} statements"));
    });

    ui.separator();

    let mut preview = sql.to_owned();
    ui.add(
        TextEdit::multiline(&mut preview)
            .font(egui::TextStyle::Monospace)
            .desired_rows(30)
            .desired_width(f32::INFINITY)
            .interactive(false),
    );
}
