//! Bottom-right notification. Successes fade out on their own; errors stay
//! until dismissed.

use std::time::{Duration, Instant};

use eframe::egui::{self, CornerRadius, Frame, Margin, RichText, Stroke};

use super::widgets;
use crate::theme::Palette;

const DURATION: Duration = Duration::from_secs(4);

pub struct Toast {
    text: String,
    is_error: bool,
    shown_at: Instant,
}

impl Toast {
    pub fn new(text: String, is_error: bool) -> Self {
        Self {
            text,
            is_error,
            shown_at: Instant::now(),
        }
    }

    /// Draws the toast. Returns false once it should go away (timed out or
    /// dismissed).
    pub fn show(&self, ui: &mut egui::Ui, p: &Palette) -> bool {
        if !self.is_error {
            let age = self.shown_at.elapsed();
            if age >= DURATION {
                return false;
            }
            ui.ctx().request_repaint_after(DURATION - age);
        }

        let color = if self.is_error { p.danger } else { p.success };
        let mut dismissed = false;
        egui::Area::new(egui::Id::unique("ferrodock-toast"))
            .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-18.0, -18.0))
            .show(ui.ctx(), |ui| {
                Frame::new()
                    .fill(p.raised)
                    .stroke(Stroke::new(1.0, p.border))
                    .corner_radius(CornerRadius::same(8))
                    .inner_margin(Margin::symmetric(14, 10))
                    .show(ui, |ui| {
                        ui.set_max_width(420.0);
                        ui.horizontal(|ui| {
                            widgets::dot(ui, color);
                            let text = ui.label(RichText::new(&self.text).color(p.text));
                            // Read out when it appears, without moving focus:
                            // errors at once, the rest when the reader is free.
                            let live = if self.is_error {
                                egui::accesskit::Live::Assertive
                            } else {
                                egui::accesskit::Live::Polite
                            };
                            ui.ctx().accesskit_node_builder(text.id, |node| {
                                node.set_role(if self.is_error {
                                    egui::accesskit::Role::Alert
                                } else {
                                    egui::accesskit::Role::Status
                                });
                                node.set_live(live);
                            });
                            dismissed =
                                widgets::icon_button(ui, super::icons::Icon::Close, "Dismiss")
                                    .clicked();
                        });
                    });
            });
        !dismissed
    }
}
