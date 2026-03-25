//! Row diff rendering with column-level highlighting, filtering, and summary.

use egui::{Color32, Grid, RichText, ScrollArea, Ui};

use crate::db::differ::filter_data_diffs;
use crate::db::types::{SemanticChange, TableDataDiff};
use crate::state::workspace::{DiffDisplayMode, DiffState};
use crate::ui::progress;

/// Renders row-level diff results with enhanced intelligence features.
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

    // Apply filter to data diffs
    let filtered_diffs = filter_data_diffs(&result.data_diffs, &diff_state.filter);

    // Summary stats
    render_summary_bar(ui, result, &filtered_diffs);
    ui.separator();

    // Semantic changes panel
    if diff_state.show_semantic && !result.semantic_changes.is_empty() {
        render_semantic_changes(ui, &result.semantic_changes);
        ui.separator();
    }

    // Filter controls
    render_filter_controls(ui, diff_state);
    ui.separator();

    // Display mode selector
    ui.horizontal(|ui| {
        ui.selectable_value(&mut diff_state.display_mode, DiffDisplayMode::Grid, "Grid");
        ui.selectable_value(
            &mut diff_state.display_mode,
            DiffDisplayMode::Unified,
            "Unified",
        );
        ui.separator();
        ui.checkbox(&mut diff_state.show_semantic, "Show semantic analysis");
    });

    ui.separator();

    // Table selector
    ui.horizontal_wrapped(|ui| {
        for table in &filtered_diffs {
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
        .and_then(|name| filtered_diffs.iter().find(|diff| &diff.table_name == name))
        .or_else(|| filtered_diffs.first());

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

fn render_summary_bar(
    ui: &mut Ui,
    result: &crate::db::differ::DatabaseDiff,
    filtered_diffs: &[TableDataDiff],
) {
    let summary = &result.summary;
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
        ui.colored_label(
            Color32::GREEN,
            format!("+{} rows", summary.total_rows_added),
        );
        ui.colored_label(
            Color32::RED,
            format!("-{} rows", summary.total_rows_removed),
        );
        ui.colored_label(
            Color32::YELLOW,
            format!("~{} rows", summary.total_rows_modified),
        );
        ui.label(format!("={} unchanged", summary.total_rows_unchanged));
        if summary.total_cells_changed > 0 {
            ui.label(format!("({} cells changed)", summary.total_cells_changed));
        }
        if has_schema_changes {
            ui.colored_label(Color32::YELLOW, "• schema changes");
        }
        ui.label(format!("{} tables compared", filtered_diffs.len()));
        if summary.tables_added > 0 {
            ui.colored_label(Color32::GREEN, format!("+{} tables", summary.tables_added));
        }
        if summary.tables_removed > 0 {
            ui.colored_label(Color32::RED, format!("-{} tables", summary.tables_removed));
        }
    });
}

fn render_semantic_changes(ui: &mut Ui, changes: &[SemanticChange]) {
    egui::CollapsingHeader::new(
        RichText::new(format!("Semantic Analysis ({} findings)", changes.len()))
            .color(Color32::from_rgb(100, 180, 255)),
    )
    .default_open(true)
    .show(ui, |ui| {
        for change in changes {
            match change {
                SemanticChange::TableRename {
                    left_name,
                    right_name,
                    confidence,
                } => {
                    ui.horizontal(|ui| {
                        ui.label("⟳");
                        ui.colored_label(
                            Color32::from_rgb(100, 180, 255),
                            format!(
                                "Table rename: {} → {} (confidence: {}%)",
                                left_name, right_name, confidence
                            ),
                        );
                    });
                }
                SemanticChange::ColumnRename {
                    table_name,
                    left_column,
                    right_column,
                    confidence,
                } => {
                    ui.horizontal(|ui| {
                        ui.label("⟳");
                        ui.colored_label(
                            Color32::from_rgb(100, 180, 255),
                            format!(
                                "Column rename in {}: {} → {} (confidence: {}%)",
                                table_name, left_column, right_column, confidence
                            ),
                        );
                    });
                }
                SemanticChange::CompatibleTypeShift {
                    table_name,
                    column_name,
                    left_type,
                    right_type,
                } => {
                    ui.horizontal(|ui| {
                        ui.label("≈");
                        ui.colored_label(
                            Color32::from_rgb(180, 200, 100),
                            format!(
                                "Compatible type shift in {}.{}: {} → {}",
                                table_name, column_name, left_type, right_type
                            ),
                        );
                    });
                }
            }
        }
    });
}

fn render_filter_controls(ui: &mut Ui, diff_state: &mut DiffState) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("Filter:").strong());
        ui.checkbox(&mut diff_state.filter.show_added, "Added");
        ui.checkbox(&mut diff_state.filter.show_removed, "Removed");
        ui.checkbox(&mut diff_state.filter.show_modified, "Modified");
    });
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
        // Removed rows
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

        // Added rows
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

        // Modified rows with column-level highlighting
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

                            // Column-level change highlighting
                            for column in &table_diff.columns {
                                let change =
                                    row.changes.iter().find(|change| change.column == *column);
                                if let Some(change) = change {
                                    // Highlight changed columns with distinct old→new display
                                    ui.vertical(|ui| {
                                        ui.colored_label(
                                            Color32::from_rgb(255, 100, 100),
                                            format!("⊖ {}", change.old_value.display()),
                                        );
                                        ui.colored_label(
                                            Color32::from_rgb(100, 255, 100),
                                            format!("⊕ {}", change.new_value.display()),
                                        );
                                    });
                                } else {
                                    // Unchanged column: dimmed
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
                        ui.horizontal(|ui| {
                            ui.label(format!("  {}:", change.column));
                            ui.colored_label(
                                Color32::from_rgb(255, 100, 100),
                                change.old_value.display(),
                            );
                            ui.label("→");
                            ui.colored_label(
                                Color32::from_rgb(100, 255, 100),
                                change.new_value.display(),
                            );
                        });
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
