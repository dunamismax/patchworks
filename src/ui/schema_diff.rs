//! Schema diff tree rendering.

use egui::{Color32, RichText, ScrollArea, Ui};

use crate::db::types::SchemaDiff;

/// Renders schema diff results.
pub fn render_schema_diff(ui: &mut Ui, schema_diff: &SchemaDiff) {
    ui.heading("Schema Diff");

    let has_changes = !schema_diff.added_tables.is_empty()
        || !schema_diff.removed_tables.is_empty()
        || !schema_diff.modified_tables.is_empty()
        || !schema_diff.added_indexes.is_empty()
        || !schema_diff.removed_indexes.is_empty()
        || !schema_diff.modified_indexes.is_empty()
        || !schema_diff.added_triggers.is_empty()
        || !schema_diff.removed_triggers.is_empty()
        || !schema_diff.modified_triggers.is_empty();

    if !has_changes {
        ui.label("No schema differences found.");
        if !schema_diff.unchanged_tables.is_empty() {
            ui.add_space(4.0);
            ui.label(format!(
                "{} tables unchanged.",
                schema_diff.unchanged_tables.len()
            ));
        }
        return;
    }

    // Summary bar
    ui.horizontal_wrapped(|ui| {
        if !schema_diff.added_tables.is_empty() {
            ui.colored_label(
                Color32::GREEN,
                format!("+{} tables", schema_diff.added_tables.len()),
            );
        }
        if !schema_diff.removed_tables.is_empty() {
            ui.colored_label(
                Color32::RED,
                format!("-{} tables", schema_diff.removed_tables.len()),
            );
        }
        if !schema_diff.modified_tables.is_empty() {
            ui.colored_label(
                Color32::YELLOW,
                format!("~{} tables", schema_diff.modified_tables.len()),
            );
        }
        if !schema_diff.added_indexes.is_empty()
            || !schema_diff.removed_indexes.is_empty()
            || !schema_diff.modified_indexes.is_empty()
        {
            let idx_count = schema_diff.added_indexes.len()
                + schema_diff.removed_indexes.len()
                + schema_diff.modified_indexes.len();
            ui.colored_label(Color32::YELLOW, format!("{idx_count} index changes"));
        }
        if !schema_diff.added_triggers.is_empty()
            || !schema_diff.removed_triggers.is_empty()
            || !schema_diff.modified_triggers.is_empty()
        {
            let trig_count = schema_diff.added_triggers.len()
                + schema_diff.removed_triggers.len()
                + schema_diff.modified_triggers.len();
            ui.colored_label(Color32::YELLOW, format!("{trig_count} trigger changes"));
        }
    });
    ui.separator();

    ScrollArea::vertical().show(ui, |ui| {
        // Tables
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
            egui::CollapsingHeader::new(
                RichText::new(format!("- {}", table.name)).color(Color32::RED),
            )
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
                        format!("~ {}: {} → {}", left.name, left.col_type, right.col_type),
                    );
                }
            });
        }

        // Indexes
        if !schema_diff.added_indexes.is_empty()
            || !schema_diff.removed_indexes.is_empty()
            || !schema_diff.modified_indexes.is_empty()
        {
            ui.separator();
            ui.label(RichText::new("Indexes").strong());
            for index in &schema_diff.added_indexes {
                ui.colored_label(
                    Color32::GREEN,
                    format!("+ {} (on {})", index.name, index.table_name),
                );
            }
            for index in &schema_diff.removed_indexes {
                ui.colored_label(
                    Color32::RED,
                    format!("- {} (on {})", index.name, index.table_name),
                );
            }
            for (left, right) in &schema_diff.modified_indexes {
                ui.colored_label(
                    Color32::YELLOW,
                    format!("~ {} (on {})", left.name, left.table_name),
                );
                if let Some(sql) = &right.create_sql {
                    ui.label(format!("  → {sql}"));
                }
            }
        }

        // Triggers
        if !schema_diff.added_triggers.is_empty()
            || !schema_diff.removed_triggers.is_empty()
            || !schema_diff.modified_triggers.is_empty()
        {
            ui.separator();
            ui.label(RichText::new("Triggers").strong());
            for trigger in &schema_diff.added_triggers {
                ui.colored_label(
                    Color32::GREEN,
                    format!("+ {} (on {})", trigger.name, trigger.table_name),
                );
            }
            for trigger in &schema_diff.removed_triggers {
                ui.colored_label(
                    Color32::RED,
                    format!("- {} (on {})", trigger.name, trigger.table_name),
                );
            }
            for (left, right) in &schema_diff.modified_triggers {
                ui.colored_label(
                    Color32::YELLOW,
                    format!("~ {} (on {})", left.name, left.table_name),
                );
                if let Some(sql) = &right.create_sql {
                    ui.label(format!("  → {sql}"));
                }
            }
        }

        // Unchanged tables
        if !schema_diff.unchanged_tables.is_empty() {
            ui.separator();
            egui::CollapsingHeader::new(format!(
                "Unchanged ({} tables)",
                schema_diff.unchanged_tables.len()
            ))
            .default_open(false)
            .show(ui, |ui| {
                for table in &schema_diff.unchanged_tables {
                    ui.label(table);
                }
            });
        }
    });
}
