//! Modal "Are you sure?" dialog for destructive actions.

use eframe::egui::{self, Align, CornerRadius, Frame, Layout, Margin, RichText, Stroke};

use super::widgets;
use crate::app::page::Confirmation;
use crate::theme::Palette;

pub enum Choice {
    Confirm,
    Cancel,
}

/// Draws the dialog over the whole window. Returns the user's choice once
/// they make one; Escape or clicking outside counts as cancel.
pub fn show(ui: &mut egui::Ui, p: &Palette, c: &Confirmation) -> Option<Choice> {
    let mut choice = None;
    let modal = egui::Modal::new(egui::Id::unique("ironstack-confirm"))
        .frame(
            Frame::new()
                .fill(p.sidebar)
                .stroke(Stroke::new(1.0, p.border))
                .corner_radius(CornerRadius::same(10))
                .inner_margin(Margin::same(20)),
        )
        .show(ui.ctx(), |ui| {
            ui.set_width(380.0);
            ui.label(RichText::new(&c.title).size(17.0).strong().color(p.text));
            ui.add_space(8.0);
            ui.label(RichText::new(&c.message).color(p.muted));
            ui.add_space(18.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if widgets::danger_button(ui, p, &c.action_label).clicked() {
                    choice = Some(Choice::Confirm);
                }
                if ui.button("Cancel").clicked() {
                    choice = Some(Choice::Cancel);
                }
            });
        });
    if choice.is_none() && modal.should_close() {
        choice = Some(Choice::Cancel);
    }
    choice
}
