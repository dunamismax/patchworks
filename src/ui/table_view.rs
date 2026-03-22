//! Table data viewer UI.

use egui::{Color32, Grid, RichText, ScrollArea, Ui};

use crate::db::types::{SortDirection, SqlValue, TableSort};
use crate::state::workspace::DatabasePaneState;
use crate::ui::progress;

/// Renders the currently selected table page.
pub fn render_table_view(ui: &mut Ui, pane: &mut DatabasePaneState) -> bool {
    let mut query_changed = false;
    if pane.is_loading {
        if let Some(progress_state) = &pane.progress {
            progress::render_progress(ui, progress_state);
        } else {
            ui.label("Loading database...");
        }
    } else if pane.is_loading_table {
        if let Some(progress_state) = &pane.progress {
            progress::render_progress(ui, progress_state);
        } else {
            ui.label("Loading table...");
        }
    } else if let Some(table_page) = &pane.table_page {
        ui.horizontal(|ui| {
            let page_count = ((table_page.total_rows as f32 / table_page.page_size as f32).ceil()
                as usize)
                .max(1);
            ui.label(format!("Page {} of {}", table_page.page + 1, page_count));
            if ui.button("Prev").clicked() && pane.table_query.page > 0 {
                pane.table_query.page -= 1;
                query_changed = true;
            }
            if ui.button("Next").clicked()
                && (pane.table_query.page + 1) * pane.table_query.page_size
                    < table_page.total_rows as usize
            {
                pane.table_query.page += 1;
                query_changed = true;
            }
        });

        ScrollArea::both().show(ui, |ui| {
            Grid::new(format!("table-grid-{}", table_page.table_name))
                .striped(true)
                .show(ui, |ui| {
                    ui.label(RichText::new("#").strong());
                    for column in &table_page.columns {
                        let current = pane.table_query.sort.as_ref().and_then(|sort| {
                            if sort.column == column.name {
                                Some(sort.direction)
                            } else {
                                None
                            }
                        });
                        let arrow = match current {
                            Some(SortDirection::Asc) => " ↑",
                            Some(SortDirection::Desc) => " ↓",
                            None => "",
                        };
                        if ui.button(format!("{}{}", column.name, arrow)).clicked() {
                            pane.table_query.sort = Some(TableSort {
                                column: column.name.clone(),
                                direction: match current {
                                    Some(SortDirection::Asc) => SortDirection::Desc,
                                    _ => SortDirection::Asc,
                                },
                            });
                            query_changed = true;
                        }
                    }
                    ui.end_row();

                    for (row_index, row) in table_page.rows.iter().enumerate() {
                        ui.label(
                            (row_index + 1 + (table_page.page * table_page.page_size)).to_string(),
                        );
                        for value in row {
                            render_cell(ui, value);
                        }
                        ui.end_row();
                    }
                });
        });
    } else {
        ui.label("Select a table to inspect.");
    }

    query_changed
}

fn render_cell(ui: &mut Ui, value: &SqlValue) {
    match value {
        SqlValue::Null => {
            ui.colored_label(Color32::GRAY, "NULL");
        }
        _ => {
            ui.label(value.display());
        }
    }
}
