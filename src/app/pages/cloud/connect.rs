//! The connect panel: what to paste into each tool so it talks to the
//! emulator instead of AWS.

use std::time::{Duration, Instant};

use eframe::egui::{self, RichText};

use crate::app::components::widgets;
use crate::docker::cloud::SNIPPETS;
use crate::theme::Palette;

/// How long the Copy button says "Copied".
const COPIED_FOR: Duration = Duration::from_secs(2);

/// The command that shows it works.
const TRY: &str = "aws sts get-caller-identity";

/// What was last copied (a snippet's title), and when.
pub type Copied = Option<(&'static str, Instant)>;

/// Text in a box, with a button that copies it.
fn code(ui: &mut egui::Ui, p: &Palette, text: &str, copied: &mut Copied, id: &'static str) {
    egui::Frame::new()
        .fill(p.bg)
        .stroke(egui::Stroke::new(1.0, p.border))
        .corner_radius(egui::CornerRadius::same(8))
        .inner_margin(egui::Margin::symmetric(12, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_top(|ui| {
                ui.label(RichText::new(text).monospace().size(12.5).color(p.text));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                    let just =
                        copied.is_some_and(|(what, at)| what == id && at.elapsed() < COPIED_FOR);
                    if just {
                        ui.ctx().request_repaint_after(COPIED_FOR);
                    }
                    if ui
                        .small_button(if just { "Copied" } else { "Copy" })
                        .clicked()
                    {
                        ui.ctx().copy_text(text.to_owned());
                        *copied = Some((id, Instant::now()));
                    }
                });
            });
        });
}

pub fn show(ui: &mut egui::Ui, p: &Palette, port: u16, selected: &mut usize, copied: &mut Copied) {
    *selected = (*selected).min(SNIPPETS.len() - 1);
    widgets::segmented(ui, p, |ui| {
        for (i, snippet) in SNIPPETS.iter().enumerate() {
            if widgets::tab(ui, p, snippet.title, None, i == *selected).clicked() {
                *selected = i;
            }
        }
    });
    ui.add_space(8.0);
    let snippet = &SNIPPETS[*selected];
    code(ui, p, &snippet.text(port), copied, snippet.title);
    ui.add_space(4.0);
    ui.label(RichText::new(snippet.hint).size(12.0).color(p.muted));
    ui.add_space(10.0);
    ui.label(RichText::new("Then try it:").size(12.5).color(p.muted));
    ui.add_space(4.0);
    code(ui, p, TRY, copied, "try");
}
