//! The activity log: every notice Ferrodock showed as a toast, newest
//! first, so an error that faded or was dismissed can still be read.

use std::collections::VecDeque;
use std::time::{SystemTime, UNIX_EPOCH};

use eframe::egui::{self, CornerRadius, Frame, Margin, RichText, Stroke};

use super::components::widgets;
use crate::docker::time_ago;
use crate::theme::Palette;

/// Entries kept; the oldest go first.
const KEPT: usize = 100;

pub struct Entry {
    /// Unix seconds.
    at: i64,
    text: String,
    is_error: bool,
}

#[derive(Default)]
pub struct Activity {
    entries: VecDeque<Entry>,
    /// Errors since the log was last opened.
    unread_errors: usize,
    pub open: bool,
}

impl Activity {
    pub fn push(&mut self, text: &str, is_error: bool) {
        let at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        self.entries.push_front(Entry {
            at,
            text: text.to_owned(),
            is_error,
        });
        self.entries.truncate(KEPT);
        if is_error {
            self.unread_errors += 1;
        }
    }

    /// For the sidebar: how many errors came since the log was opened.
    pub fn badge(&self) -> String {
        match self.unread_errors {
            0 => String::new(),
            n => n.to_string(),
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui, p: &Palette) {
        if !self.open {
            return;
        }
        self.unread_errors = 0;
        let mut close = false;
        let modal = egui::Modal::new(egui::Id::unique("ferrodock-activity"))
            .frame(
                Frame::new()
                    .fill(p.sidebar)
                    .stroke(Stroke::new(1.0, p.border))
                    .corner_radius(CornerRadius::same(12))
                    .inner_margin(Margin::same(20)),
            )
            .show(ui.ctx(), |ui| {
                ui.set_width(520.0);
                ui.label(
                    RichText::new("Activity")
                        .font(egui::FontId::new(18.0, crate::theme::semibold()))
                        .color(p.text),
                );
                ui.label(
                    RichText::new("What Ferrodock did and what went wrong, newest first.")
                        .color(p.muted),
                );
                ui.add_space(10.0);
                if self.entries.is_empty() {
                    ui.label(RichText::new("Nothing yet.").color(p.muted));
                }
                egui::ScrollArea::vertical()
                    .max_height(360.0)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        for entry in &self.entries {
                            ui.horizontal_top(|ui| {
                                let color = if entry.is_error { p.danger } else { p.success };
                                ui.add_space(2.0);
                                widgets::dot(ui, color);
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(&entry.text).color(if entry.is_error {
                                            p.danger
                                        } else {
                                            p.text
                                        }),
                                    )
                                    .wrap(),
                                );
                            });
                            ui.label(RichText::new(time_ago(entry.at)).size(11.5).color(p.muted));
                            ui.add_space(6.0);
                        }
                    });
                ui.add_space(10.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if widgets::primary_button(ui, "Done").clicked() {
                        close = true;
                    }
                    if !self.entries.is_empty() {
                        if ui.button("Clear").clicked() {
                            self.entries.clear();
                        }
                        if ui.button("Copy").clicked() {
                            let text: Vec<String> =
                                self.entries.iter().map(|e| e.text.clone()).collect();
                            ui.ctx().copy_text(text.join("\n"));
                        }
                    }
                });
            });
        if close || modal.should_close() {
            self.open = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_the_newest_and_counts_unread_errors() {
        let mut log = Activity::default();
        for i in 0..KEPT + 5 {
            log.push(&format!("note {i}"), i % 50 == 0);
        }
        assert_eq!(log.entries.len(), KEPT);
        assert_eq!(
            log.entries.front().unwrap().text,
            format!("note {}", KEPT + 4)
        );
        assert_eq!(log.badge(), "3");
    }
}
