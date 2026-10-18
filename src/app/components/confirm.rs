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
pub fn show(ui: &mut egui::Ui, p: &Palette, c: &mut Confirmation) -> Option<Choice> {
    let mut choice = None;
    let modal = egui::Modal::new(egui::Id::unique("ferrodock-confirm"))
        .frame(
            Frame::new()
                .fill(p.sidebar)
                .stroke(Stroke::new(1.0, p.border))
                .corner_radius(CornerRadius::same(10))
                .inner_margin(Margin::same(20)),
        )
        .show(ui.ctx(), |ui| {
            ui.set_width(if c.typed.is_some() { 440.0 } else { 380.0 });
            ui.label(RichText::new(&c.title).size(17.0).strong().color(p.text));
            ui.add_space(8.0);
            ui.label(RichText::new(&c.message).color(p.muted));
            if let Some(option) = &mut c.option {
                ui.add_space(12.0);
                ui.checkbox(
                    &mut option.on,
                    RichText::new(&option.label).strong().color(p.text),
                );
                if !option.hint.is_empty() {
                    ui.horizontal(|ui| {
                        ui.add_space(24.0);
                        ui.label(RichText::new(&option.hint).size(12.0).color(p.muted));
                    });
                }
            }
            if let Some(typed) = &mut c.typed {
                ui.add_space(12.0);
                let label = ui.label(
                    RichText::new(format!("Type “{}” to confirm", typed.word)).color(p.text),
                );
                let field = ui
                    .add(
                        egui::TextEdit::singleline(&mut typed.entered)
                            .hint_text(&typed.word)
                            .desired_width(f32::INFINITY),
                    )
                    .labelled_by(label.id);
                if typed.entered.is_empty() && !field.has_focus() && !field.lost_focus() {
                    field.request_focus();
                }
                if field.lost_focus()
                    && ui.input(|i| i.key_pressed(egui::Key::Enter))
                    && typed.matches()
                {
                    choice = Some(Choice::Confirm);
                }
            }
            ui.add_space(18.0);
            let ready = c.ready();
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let button = ui
                    .add_enabled_ui(ready, |ui| widgets::danger_button(ui, p, &c.action_label))
                    .inner;
                if button.clicked() {
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
