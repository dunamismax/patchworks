//! Schema diff tree rendering.

use egui::{Color32, RichText, Ui};

use crate::db::types::SchemaDiff;

/// Renders schema diff results.
pub fn render_schema_diff(ui: &mut Ui, schema_diff: &SchemaDiff) {
    ui.heading("Schema Diff");

    for table in &schema_diff.added_tables {
        egui::CollapsingHeader::new(
            RichText::new(format!("+ {}", table.name)).color(Color32::GREEN),
        )
        .default_open(true)
        .show(ui, |ui| {
            for column in &table.columns {
                ui.colored_label(
                    Color32::GREEN,
                    format!("+ {} {}", column.name, column.col_type),
                );
            }
        });
    }

    for table in &schema_diff.removed_tables {
        egui::CollapsingHeader::new(RichText::new(format!("- {}", table.name)).color(Color32::RED))
            .default_open(true)
            .show(ui, |ui| {
                for column in &table.columns {
                    ui.colored_label(
                        Color32::RED,
                        format!("- {} {}", column.name, column.col_type),
                    );
                }
            });
    }

    for table in &schema_diff.modified_tables {
        egui::CollapsingHeader::new(
            RichText::new(format!("~ {}", table.table_name)).color(Color32::YELLOW),
        )
        .default_open(true)
        .show(ui, |ui| {
            for column in &table.added_columns {
                ui.colored_label(
                    Color32::GREEN,
                    format!("+ {} {}", column.name, column.col_type),
                );
            }
            for column in &table.removed_columns {
                ui.colored_label(
                    Color32::RED,
                    format!("- {} {}", column.name, column.col_type),
                );
            }
            for (left, right) in &table.modified_columns {
                ui.colored_label(
                    Color32::YELLOW,
                    format!("~ {}: {} -> {}", left.name, left.col_type, right.col_type),
                );
            }
        });
    }

    if !schema_diff.unchanged_tables.is_empty() {
        ui.separator();
        ui.label(RichText::new("Unchanged").strong());
        for table in &schema_diff.unchanged_tables {
            ui.label(table);
        }
    }
}
