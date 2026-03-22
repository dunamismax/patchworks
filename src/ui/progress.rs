//! Shared progress rendering helpers.

use egui::{ProgressBar, Spinner, Ui};

use crate::state::workspace::ProgressState;

/// Renders a background-task progress indicator with optional determinate progress.
pub fn render_progress(ui: &mut Ui, progress: &ProgressState) {
    ui.horizontal(|ui| {
        ui.add(Spinner::new());
        ui.label(&progress.label);
    });

    if let Some(fraction) = progress.fraction() {
        let mut bar = ProgressBar::new(fraction).show_percentage();
        if let Some(step_label) = progress.step_label() {
            bar = bar.text(step_label);
        }
        ui.add(bar);
    }
}
