//! Row diff rendering.

use egui::{Color32, Grid, RichText, ScrollArea, Spinner, Ui};

use crate::db::types::TableDataDiff;
use crate::state::workspace::{DiffDisplayMode, DiffState};

/// Renders row-level diff results.
pub fn render_diff_view(ui: &mut Ui, diff_state: &mut DiffState) {
    if diff_state.is_computing {
        ui.horizontal(|ui| {
            ui.add(Spinner::new());
            ui.label("Computing database diff in the background...");
        });
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
            if ui.selectable_label(selected, &table.table_name).clicked() {
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
        Grid::new(format!("diff-grid-{}", table_diff.table_name))
            .striped(true)
            .show(ui, |ui| {
                ui.label(RichText::new("Change").strong());
                for column in &table_diff.columns {
                    ui.label(RichText::new(column).strong());
                }
                ui.end_row();

                for row in &table_diff.removed_rows {
                    ui.colored_label(Color32::RED, "-");
                    for value in row {
                        ui.colored_label(Color32::RED, value.display());
                    }
                    ui.end_row();
                }

                for row in &table_diff.added_rows {
                    ui.colored_label(Color32::GREEN, "+");
                    for value in row {
                        ui.colored_label(Color32::GREEN, value.display());
                    }
                    ui.end_row();
                }

                for row in &table_diff.modified_rows {
                    ui.colored_label(Color32::YELLOW, "~");
                    for column in &table_diff.columns {
                        let change = row.changes.iter().find(|change| change.column == *column);
                        if let Some(change) = change {
                            ui.colored_label(
                                Color32::YELLOW,
                                format!(
                                    "{} -> {}",
                                    change.old_value.display(),
                                    change.new_value.display()
                                ),
                            );
                        } else {
                            ui.label("=");
                        }
                    }
                    ui.end_row();
                }
            });
    });
}

fn render_unified(ui: &mut Ui, table_diff: &TableDataDiff) {
    ScrollArea::vertical().show(ui, |ui| {
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
        for row in &table_diff.modified_rows {
            ui.colored_label(Color32::YELLOW, format!("~ {:?}", row.primary_key));
            for change in &row.changes {
                ui.label(format!(
                    "  {}: {} -> {}",
                    change.column,
                    change.old_value.display(),
                    change.new_value.display()
                ));
            }
        }
    });
}
