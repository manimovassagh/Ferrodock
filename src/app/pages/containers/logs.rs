//! Logs tab: the container's recent output, following new lines.

use eframe::egui::{self, Align, CornerRadius, Layout, RichText, Stroke, TextStyle};

use crate::app::store::Live;
use crate::theme::Palette;

pub(super) fn show(ui: &mut egui::Ui, p: &Palette, live: Option<&Live>, filter: &mut String) {
    let Some(live) = live else {
        return;
    };
    let needle = filter.trim().to_lowercase();
    let lines: Vec<&str> = live
        .logs
        .lines()
        .filter(|line| needle.is_empty() || line.to_lowercase().contains(&needle))
        .collect();

    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(filter)
                .hint_text("Filter logs")
                .desired_width(160.0),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui
                .small_button("Copy")
                .on_hover_text("Copy the shown lines")
                .clicked()
            {
                ui.ctx().copy_text(lines.join("\n"));
            }
            ui.label(
                RichText::new(format!("{} lines", lines.len()))
                    .size(12.0)
                    .color(p.muted),
            );
        });
    });
    ui.add_space(6.0);

    egui::Frame::new()
        .fill(p.bg)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            if lines.is_empty() {
                let message = if live.logs.is_empty() {
                    "No log output yet."
                } else {
                    "No lines match the filter."
                };
                ui.label(RichText::new(message).color(p.muted));
                ui.allocate_space(ui.available_size());
                return;
            }
            let row_height = ui.text_style_height(&TextStyle::Monospace);
            egui::ScrollArea::both()
                .auto_shrink(false)
                .stick_to_bottom(true)
                .show_rows(ui, row_height, lines.len(), |ui, range| {
                    for line in &lines[range] {
                        ui.label(RichText::new(*line).monospace().size(12.0).color(p.text));
                    }
                });
        });
}
