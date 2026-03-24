//! Schema browser: tables, views, indexes, and triggers with DDL preview.

use egui::{Color32, RichText, ScrollArea, TextEdit, Ui};

use crate::db::types::DatabaseSummary;

/// Renders the schema browser for one or both loaded databases.
pub fn render_schema_browser(
    ui: &mut Ui,
    left: Option<&DatabaseSummary>,
    right: Option<&DatabaseSummary>,
) {
    ui.heading("Schema Browser");

    if left.is_none() && right.is_none() {
        ui.label("Load a database to browse its schema.");
        return;
    }

    ScrollArea::vertical().show(ui, |ui| {
        if let Some(summary) = left {
            render_database_schema(ui, summary, "Left");
        }
        if let Some(summary) = right {
            if left.is_some() {
                ui.separator();
            }
            render_database_schema(ui, summary, "Right");
        }
    });
}

fn render_database_schema(ui: &mut Ui, summary: &DatabaseSummary, label: &str) {
    ui.label(RichText::new(format!("{label}: {}", summary.path)).strong());
    ui.add_space(4.0);

    // Tables
    if !summary.tables.is_empty() {
        egui::CollapsingHeader::new(
            RichText::new(format!("Tables ({})", summary.tables.len())).strong(),
        )
        .default_open(true)
        .show(ui, |ui| {
            for table in &summary.tables {
                egui::CollapsingHeader::new(format!(
                    "{} ({} rows, {} columns)",
                    table.name,
                    table.row_count,
                    table.columns.len()
                ))
                .id_salt(format!("{label}-table-{}", table.name))
                .show(ui, |ui| {
                    // Column details
                    for col in &table.columns {
                        let pk_marker = if col.is_primary_key { " 🔑" } else { "" };
                        let nullable = if col.nullable { "" } else { " NOT NULL" };
                        let default = col
                            .default_value
                            .as_ref()
                            .map(|d| format!(" DEFAULT {d}"))
                            .unwrap_or_default();
                        ui.label(format!(
                            "  {} {}{}{}{}",
                            col.name, col.col_type, nullable, default, pk_marker
                        ));
                    }
                    // DDL preview
                    if let Some(sql) = &table.create_sql {
                        ui.add_space(4.0);
                        render_ddl_block(ui, sql);
                    }
                });
            }
        });
    }

    // Views
    if !summary.views.is_empty() {
        ui.add_space(4.0);
        egui::CollapsingHeader::new(
            RichText::new(format!("Views ({})", summary.views.len())).strong(),
        )
        .default_open(true)
        .show(ui, |ui| {
            for view in &summary.views {
                egui::CollapsingHeader::new(&view.name)
                    .id_salt(format!("{label}-view-{}", view.name))
                    .show(ui, |ui| {
                        if let Some(sql) = &view.create_sql {
                            render_ddl_block(ui, sql);
                        } else {
                            ui.colored_label(Color32::GRAY, "(no DDL available)");
                        }
                    });
            }
        });
    }

    // Indexes
    if !summary.indexes.is_empty() {
        ui.add_space(4.0);
        egui::CollapsingHeader::new(
            RichText::new(format!("Indexes ({})", summary.indexes.len())).strong(),
        )
        .default_open(false)
        .show(ui, |ui| {
            for index in &summary.indexes {
                egui::CollapsingHeader::new(format!("{} → {}", index.name, index.table_name))
                    .id_salt(format!("{label}-idx-{}", index.name))
                    .show(ui, |ui| {
                        if let Some(sql) = &index.create_sql {
                            render_ddl_block(ui, sql);
                        } else {
                            ui.colored_label(Color32::GRAY, "(no DDL available)");
                        }
                    });
            }
        });
    }

    // Triggers
    if !summary.triggers.is_empty() {
        ui.add_space(4.0);
        egui::CollapsingHeader::new(
            RichText::new(format!("Triggers ({})", summary.triggers.len())).strong(),
        )
        .default_open(false)
        .show(ui, |ui| {
            for trigger in &summary.triggers {
                egui::CollapsingHeader::new(format!("{} → {}", trigger.name, trigger.table_name))
                    .id_salt(format!("{label}-trig-{}", trigger.name))
                    .show(ui, |ui| {
                        if let Some(sql) = &trigger.create_sql {
                            render_ddl_block(ui, sql);
                        } else {
                            ui.colored_label(Color32::GRAY, "(no DDL available)");
                        }
                    });
            }
        });
    }
}

fn render_ddl_block(ui: &mut Ui, sql: &str) {
    let mut text = sql.to_owned();
    ui.add(
        TextEdit::multiline(&mut text)
            .font(egui::TextStyle::Monospace)
            .desired_rows(sql.lines().count().clamp(2, 12))
            .desired_width(f32::INFINITY)
            .interactive(false),
    );
}
