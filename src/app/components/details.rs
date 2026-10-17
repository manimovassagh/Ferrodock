//! Building blocks for the right-hand details pane.

use eframe::egui::{self, Align, CornerRadius, Layout, RichText, Stroke};

use super::widgets;
use crate::theme::Palette;

/// Title row with a close button. Returns true when close was clicked.
pub fn header(ui: &mut egui::Ui, p: &Palette, title: &str) -> bool {
    let mut closed = false;
    ui.horizontal(|ui| {
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            closed = widgets::icon_button(ui, "✕", "Close").clicked();
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                ui.add(
                    egui::Label::new(RichText::new(title).size(18.0).strong().color(p.text))
                        .truncate(),
                );
            });
        });
    });
    closed
}

/// A titled card holding label/value rows. Fill it with [`field`] and
/// [`copy_field`].
pub fn section(
    ui: &mut egui::Ui,
    p: &Palette,
    title: &str,
    add_fields: impl FnOnce(&mut egui::Ui),
) {
    ui.label(
        RichText::new(title.to_uppercase())
            .size(11.0)
            .color(p.muted),
    );
    ui.add_space(2.0);
    egui::Frame::new()
        .fill(p.bg)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(egui::Margin::same(12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            egui::Grid::new(title)
                .num_columns(2)
                .spacing([14.0, 8.0])
                .show(ui, add_fields);
        });
    ui.add_space(14.0);
}

/// One label/value row. Empty values show as a dash.
pub fn field(ui: &mut egui::Ui, p: &Palette, label: &str, value: &str) {
    ui.label(RichText::new(label).color(p.muted));
    let value = if value.is_empty() { "—" } else { value };
    ui.add(egui::Label::new(RichText::new(value).color(p.text)).wrap());
    ui.end_row();
}

/// A row showing `short` with a button that copies `full`.
pub fn copy_field(ui: &mut egui::Ui, p: &Palette, label: &str, short: &str, full: &str) {
    ui.label(RichText::new(label).color(p.muted));
    ui.horizontal(|ui| {
        ui.label(RichText::new(short).monospace().color(p.text))
            .on_hover_text(full);
        if ui
            .small_button("Copy")
            .on_hover_text("Copy to clipboard")
            .clicked()
        {
            ui.ctx().copy_text(full.to_owned());
        }
    });
    ui.end_row();
}
