//! Logs tab: the container's recent output, following new lines, with
//! colours, timestamps, search highlighting and saving to a file.

use eframe::egui::{self, Align, CornerRadius, FontId, Layout, RichText, Stroke};

use crate::app::components::{ansi, icons::Icon, widgets};
use crate::app::page::Intent;
use crate::app::store::Live;
use crate::theme::Palette;

/// How the log is shown. Kept per page, so it survives switching tabs.
#[derive(Default)]
pub(super) struct LogView {
    pub filter: String,
    timestamps: bool,
    wrap: bool,
    /// Scroll to the newest line on the next frame.
    jump: bool,
}

/// Docker's RFC 3339 timestamp at the start of a line, and the rest.
fn split_timestamp(line: &str) -> (Option<&str>, &str) {
    match line.split_once(' ') {
        Some((ts, rest))
            if ts.len() >= 20 && ts.as_bytes()[4] == b'-' && ts.as_bytes()[10] == b'T' =>
        {
            (Some(ts), rest)
        }
        _ => (None, line),
    }
}

/// `2026-10-17T15:04:05.123Z` → `15:04:05`.
fn short_time(ts: &str) -> &str {
    ts.get(11..19).unwrap_or(ts)
}

pub(super) fn show(
    ui: &mut egui::Ui,
    p: &Palette,
    live: Option<&Live>,
    view: &mut LogView,
    container: &str,
) -> Option<Intent> {
    let live = live?;
    let needle = view.filter.trim().to_lowercase();
    let lines: Vec<(Option<&str>, &str)> = live
        .logs
        .lines()
        .map(split_timestamp)
        .filter(|(_, text)| needle.is_empty() || ansi::strip(text).to_lowercase().contains(&needle))
        .collect();
    let mut intent = None;

    ui.add(
        egui::TextEdit::singleline(&mut view.filter)
            .hint_text("Filter logs")
            .desired_width(f32::INFINITY),
    );
    ui.horizontal(|ui| {
        ui.toggle_value(&mut view.timestamps, "Time")
            .on_hover_text("Show when each line was written");
        ui.toggle_value(&mut view.wrap, "Wrap")
            .on_hover_text("Wrap long lines");
        let count = if needle.is_empty() {
            format!("{} lines", lines.len())
        } else {
            format!("{} matches", lines.len())
        };
        ui.label(RichText::new(count).size(12.0).color(p.muted));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui
                .small_button("Save")
                .on_hover_text("Save the whole log to your Downloads folder")
                .clicked()
            {
                intent = Some(save(live, container));
            }
            if ui
                .small_button("Copy")
                .on_hover_text("Copy the shown lines")
                .clicked()
            {
                let text: Vec<String> = lines.iter().map(|(_, l)| ansi::strip(l)).collect();
                ui.ctx().copy_text(text.join("\n"));
            }
        });
    });
    ui.add_space(6.0);

    let frame = egui::Frame::new()
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
                return true;
            }
            let font = FontId::monospace(12.0);
            ui.spacing_mut().item_spacing.y = 2.0;
            let row = |ui: &mut egui::Ui, (ts, text): &(Option<&str>, &str)| {
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    if view.timestamps
                        && let Some(ts) = ts
                    {
                        ui.label(
                            RichText::new(short_time(ts))
                                .font(font.clone())
                                .color(p.muted),
                        )
                        .on_hover_text(*ts);
                    }
                    let job = ansi::layout(p, text, &needle, font.clone());
                    let label = egui::Label::new(job);
                    ui.add(if view.wrap {
                        label.wrap()
                    } else {
                        label.extend()
                    });
                });
            };
            let mut area = egui::ScrollArea::both()
                .auto_shrink(false)
                .stick_to_bottom(true);
            if view.wrap {
                area = egui::ScrollArea::vertical()
                    .auto_shrink(false)
                    .stick_to_bottom(true);
            }
            if std::mem::take(&mut view.jump) {
                area = area.vertical_scroll_offset(f32::MAX);
            }
            let output = if view.wrap {
                area.show(ui, |ui| lines.iter().for_each(|line| row(ui, line)))
            } else {
                let height = ui.fonts_mut(|f| f.row_height(&font)) + 2.0;
                area.show_rows(ui, height, lines.len(), |ui, range| {
                    lines[range].iter().for_each(|line| row(ui, line))
                })
            };
            let bottom = output.state.clamped_offset().y + output.inner_rect.height();
            bottom >= output.content_size.y - 4.0
        });

    // Scrolled up: offer a way back to the live end.
    if !frame.inner {
        let rect = frame.response.rect;
        let size = egui::vec2(132.0, 30.0);
        let at =
            egui::Rect::from_min_size(rect.right_bottom() - size - egui::vec2(14.0, 14.0), size);
        let clicked = ui
            .put(at, |ui: &mut egui::Ui| {
                widgets::button(ui, Icon::Download, "Latest")
            })
            .on_hover_text("Jump to the newest line and keep following");
        if clicked.clicked() {
            view.jump = true;
        }
    }
    intent
}

/// Writes the buffered log to `~/Downloads/<container>-<time>.log`.
fn save(live: &Live, container: &str) -> Intent {
    let dir = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(|home| std::path::PathBuf::from(home).join("Downloads"))
        .filter(|d| d.is_dir())
        .unwrap_or_else(std::env::temp_dir);
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default();
    let path = dir.join(format!("{container}-{stamp}.log"));
    let text: Vec<String> = live.logs.lines().map(ansi::strip).collect();
    match std::fs::write(&path, text.join("\n") + "\n") {
        Ok(()) => Intent::Notify {
            text: format!("Saved the log to {}", path.display()),
            is_error: false,
        },
        Err(e) => Intent::Notify {
            text: format!("Couldn't save the log: {e}"),
            is_error: true,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_docker_timestamps() {
        let (ts, rest) = split_timestamp("2026-10-17T15:04:05.123456789Z GET / 200");
        assert_eq!(ts, Some("2026-10-17T15:04:05.123456789Z"));
        assert_eq!(rest, "GET / 200");
        assert_eq!(short_time(ts.unwrap()), "15:04:05");
        assert_eq!(split_timestamp("plain line"), (None, "plain line"));
    }
}
