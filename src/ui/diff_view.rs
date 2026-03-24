//! Row diff rendering.

use egui::{Color32, Grid, RichText, ScrollArea, Ui};

use crate::db::types::TableDataDiff;
use crate::state::workspace::{DiffDisplayMode, DiffState};
use crate::ui::progress;

/// Renders row-level diff results.
pub fn render_diff_view(ui: &mut Ui, diff_state: &mut DiffState) {
    if let Some(progress_state) = &diff_state.progress {
        progress::render_progress(ui, progress_state);
        ui.separator();
    } else if diff_state.is_computing {
        ui.label("Computing database diff in the background...");
        ui.separator();
    }

    if let Some(error) = &diff_state.error {
        ui.colored_label(Color32::RED, error);
    }

    let Some(result) = &diff_state.result else {
        if !diff_state.is_computing {
            ui.label("Load two databases and run a diff to see changes.");
        }
        return;
    };

    // Summary stats across all tables
    let total_added: u64 = result.data_diffs.iter().map(|d| d.stats.added).sum();
    let total_removed: u64 = result.data_diffs.iter().map(|d| d.stats.removed).sum();
    let total_modified: u64 = result.data_diffs.iter().map(|d| d.stats.modified).sum();
    let total_unchanged: u64 = result.data_diffs.iter().map(|d| d.stats.unchanged).sum();
    let schema = &result.schema;
    let has_schema_changes = !schema.added_tables.is_empty()
        || !schema.removed_tables.is_empty()
        || !schema.modified_tables.is_empty()
        || !schema.added_indexes.is_empty()
        || !schema.removed_indexes.is_empty()
        || !schema.modified_indexes.is_empty()
        || !schema.added_triggers.is_empty()
        || !schema.removed_triggers.is_empty()
        || !schema.modified_triggers.is_empty();

    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("Summary:").strong());
        ui.colored_label(Color32::GREEN, format!("+{total_added}"));
        ui.colored_label(Color32::RED, format!("-{total_removed}"));
        ui.colored_label(Color32::YELLOW, format!("~{total_modified}"));
        ui.label(format!("={total_unchanged}"));
        if has_schema_changes {
            ui.colored_label(Color32::YELLOW, "• schema changes");
        }
        ui.label(format!("{} tables compared", result.data_diffs.len()));
    });

    ui.separator();

    ui.horizontal(|ui| {
        ui.selectable_value(&mut diff_state.display_mode, DiffDisplayMode::Grid, "Grid");
        ui.selectable_value(
            &mut diff_state.display_mode,
            DiffDisplayMode::Unified,
            "Unified",
        );
    });

    ui.separator();
    ui.horizontal_wrapped(|ui| {
        for table in &result.data_diffs {
            let selected = diff_state.selected_table.as_deref() == Some(table.table_name.as_str());
            let has_changes =
                table.stats.added > 0 || table.stats.removed > 0 || table.stats.modified > 0;
            let label = if has_changes {
                format!(
                    "{} (+{}/-{}/~{})",
                    table.table_name, table.stats.added, table.stats.removed, table.stats.modified
                )
            } else {
                format!("{} ✓", table.table_name)
            };
            if ui.selectable_label(selected, label).clicked() {
                diff_state.selected_table = Some(table.table_name.clone());
            }
        }
    });

    let active = diff_state
        .selected_table
        .as_ref()
        .and_then(|name| {
            result
                .data_diffs
                .iter()
                .find(|diff| &diff.table_name == name)
        })
        .or_else(|| result.data_diffs.first());

    if let Some(table_diff) = active {
        render_stats(ui, table_diff);
        match diff_state.display_mode {
            DiffDisplayMode::Grid => render_grid(ui, table_diff),
            DiffDisplayMode::Unified => render_unified(ui, table_diff),
        }
    } else {
        ui.label("No shared tables were available for row diffing.");
    }
}

fn render_stats(ui: &mut Ui, table_diff: &TableDataDiff) {
    ui.horizontal_wrapped(|ui| {
        ui.colored_label(Color32::GREEN, format!("{} added", table_diff.stats.added));
        ui.colored_label(
            Color32::RED,
            format!("{} removed", table_diff.stats.removed),
        );
        ui.colored_label(
            Color32::YELLOW,
            format!("{} modified", table_diff.stats.modified),
        );
        ui.label(format!("{} unchanged", table_diff.stats.unchanged));
    });
    for warning in &table_diff.warnings {
        ui.colored_label(Color32::YELLOW, warning);
    }
    ui.separator();
}

fn render_grid(ui: &mut Ui, table_diff: &TableDataDiff) {
    ScrollArea::both().show(ui, |ui| {
        // Collapsible sections for each change type
        if !table_diff.removed_rows.is_empty() {
            egui::CollapsingHeader::new(
                RichText::new(format!("Removed ({} rows)", table_diff.removed_rows.len()))
                    .color(Color32::RED),
            )
            .default_open(true)
            .show(ui, |ui| {
                Grid::new(format!("diff-grid-removed-{}", table_diff.table_name))
                    .striped(true)
                    .show(ui, |ui| {
                        for column in &table_diff.columns {
                            ui.label(RichText::new(column).strong());
                        }
                        ui.end_row();

                        for row in &table_diff.removed_rows {
                            for value in row {
                                ui.colored_label(Color32::RED, value.display());
                            }
                            ui.end_row();
                        }
                    });
            });
        }

        if !table_diff.added_rows.is_empty() {
            egui::CollapsingHeader::new(
                RichText::new(format!("Added ({} rows)", table_diff.added_rows.len()))
                    .color(Color32::GREEN),
            )
            .default_open(true)
            .show(ui, |ui| {
                Grid::new(format!("diff-grid-added-{}", table_diff.table_name))
                    .striped(true)
                    .show(ui, |ui| {
                        for column in &table_diff.columns {
                            ui.label(RichText::new(column).strong());
                        }
                        ui.end_row();

                        for row in &table_diff.added_rows {
                            for value in row {
                                ui.colored_label(Color32::GREEN, value.display());
                            }
                            ui.end_row();
                        }
                    });
            });
        }

        if !table_diff.modified_rows.is_empty() {
            egui::CollapsingHeader::new(
                RichText::new(format!(
                    "Modified ({} rows)",
                    table_diff.modified_rows.len()
                ))
                .color(Color32::YELLOW),
            )
            .default_open(true)
            .show(ui, |ui| {
                Grid::new(format!("diff-grid-modified-{}", table_diff.table_name))
                    .striped(true)
                    .show(ui, |ui| {
                        ui.label(RichText::new("Key").strong());
                        for column in &table_diff.columns {
                            ui.label(RichText::new(column).strong());
                        }
                        ui.end_row();

                        for row in &table_diff.modified_rows {
                            let pk_label = row
                                .primary_key
                                .iter()
                                .map(|v| v.display())
                                .collect::<Vec<_>>()
                                .join(", ");
                            ui.label(pk_label);
                            for column in &table_diff.columns {
                                let change =
                                    row.changes.iter().find(|change| change.column == *column);
                                if let Some(change) = change {
                                    ui.colored_label(
                                        Color32::YELLOW,
                                        format!(
                                            "{} → {}",
                                            change.old_value.display(),
                                            change.new_value.display()
                                        ),
                                    );
                                } else {
                                    ui.colored_label(Color32::GRAY, "—");
                                }
                            }
                            ui.end_row();
                        }
                    });
            });
        }

        if table_diff.added_rows.is_empty()
            && table_diff.removed_rows.is_empty()
            && table_diff.modified_rows.is_empty()
        {
            ui.label("No changes in this table.");
        }
    });
}

fn render_unified(ui: &mut Ui, table_diff: &TableDataDiff) {
    ScrollArea::vertical().show(ui, |ui| {
        if !table_diff.removed_rows.is_empty() {
            egui::CollapsingHeader::new(
                RichText::new(format!("Removed ({})", table_diff.removed_rows.len()))
                    .color(Color32::RED),
            )
            .default_open(true)
            .show(ui, |ui| {
                for row in &table_diff.removed_rows {
                    ui.colored_label(
                        Color32::RED,
                        format!(
                            "- {}",
                            row.iter()
                                .map(|value| value.display())
                                .collect::<Vec<_>>()
                                .join(" | ")
                        ),
                    );
                }
            });
        }

        if !table_diff.added_rows.is_empty() {
            egui::CollapsingHeader::new(
                RichText::new(format!("Added ({})", table_diff.added_rows.len()))
                    .color(Color32::GREEN),
            )
            .default_open(true)
            .show(ui, |ui| {
                for row in &table_diff.added_rows {
                    ui.colored_label(
                        Color32::GREEN,
                        format!(
                            "+ {}",
                            row.iter()
                                .map(|value| value.display())
                                .collect::<Vec<_>>()
                                .join(" | ")
                        ),
                    );
                }
            });
        }

        if !table_diff.modified_rows.is_empty() {
            egui::CollapsingHeader::new(
                RichText::new(format!("Modified ({})", table_diff.modified_rows.len()))
                    .color(Color32::YELLOW),
            )
            .default_open(true)
            .show(ui, |ui| {
                for row in &table_diff.modified_rows {
                    let pk_label = row
                        .primary_key
                        .iter()
                        .map(|v| v.display())
                        .collect::<Vec<_>>()
                        .join(", ");
                    ui.colored_label(Color32::YELLOW, format!("~ [{pk_label}]"));
                    for change in &row.changes {
                        ui.label(format!(
                            "  {}: {} → {}",
                            change.column,
                            change.old_value.display(),
                            change.new_value.display()
                        ));
                    }
                }
            });
        }

        if table_diff.added_rows.is_empty()
            && table_diff.removed_rows.is_empty()
            && table_diff.modified_rows.is_empty()
        {
            ui.label("No changes in this table.");
        }
    });
}
