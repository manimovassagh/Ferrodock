//! The "?" overlay listing every keyboard shortcut.

use eframe::egui::{self, CornerRadius, Frame, Margin, RichText, Stroke};

use super::widgets;
use crate::theme::Palette;

/// A titled group of (keys, what they do) rows.
pub type Section = (&'static str, Vec<(String, &'static str)>);

/// Draws the overlay. Returns false once it should close.
pub fn show(ui: &mut egui::Ui, p: &Palette, sections: &[Section]) -> bool {
    let modal = egui::Modal::new(egui::Id::unique("ferrodock-shortcuts"))
        .frame(
            Frame::new()
                .fill(p.sidebar)
                .stroke(Stroke::new(1.0, p.border))
                .corner_radius(CornerRadius::same(10))
                .inner_margin(Margin::same(20)),
        )
        .show(ui.ctx(), |ui| {
            ui.set_width(420.0);
            ui.label(
                RichText::new("Keyboard shortcuts")
                    .size(17.0)
                    .strong()
                    .color(p.text),
            );
            ui.add_space(10.0);
            widgets::dialog_body(ui, |ui| {
                for (title, rows) in sections {
                    ui.label(
                        RichText::new(title.to_uppercase())
                            .size(11.0)
                            .color(p.muted),
                    );
                    ui.add_space(2.0);
                    egui::Grid::new(title)
                        .num_columns(2)
                        .spacing([18.0, 6.0])
                        .show(ui, |ui| {
                            for (keys, action) in rows {
                                ui.label(RichText::new(keys).monospace().color(p.text));
                                ui.label(RichText::new(*action).color(p.muted));
                                ui.end_row();
                            }
                        });
                    ui.add_space(12.0);
                }
            });
            ui.label(
                RichText::new("Press Esc to close")
                    .size(12.0)
                    .color(p.muted),
            );
        });
    !modal.should_close()
}
