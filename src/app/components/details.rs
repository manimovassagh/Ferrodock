//! Building blocks for the right-hand details pane.

use eframe::egui::{self, Align, CornerRadius, Layout, RichText, Stroke};

use super::icons::Icon;
use super::widgets;
use crate::theme::Palette;

/// Title row with a close button. Returns true when close was clicked.
pub fn header(ui: &mut egui::Ui, p: &Palette, title: &str) -> bool {
    let mut closed = false;
    ui.horizontal(|ui| {
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            closed = widgets::icon_button(ui, Icon::Close, "Close (Esc)").clicked();
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                ui.add(
                    egui::Label::new(
                        RichText::new(title)
                            .font(egui::FontId::new(19.0, crate::theme::semibold()))
                            .color(p.text),
                    )
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

/// How long "Copied" shows after copying, in seconds.
const COPIED_FOR: f64 = 1.5;

/// A row showing `short` in monospace, shortened in the middle when it
/// doesn't fit. Clicking it, or the Copy button, copies `full`.
pub fn copy_field(ui: &mut egui::Ui, p: &Palette, label: &str, short: &str, full: &str) {
    ui.label(RichText::new(label).color(p.muted));
    ui.horizontal(|ui| {
        let id = egui::Id::unique(("copied", label, full));
        let now = ui.input(|i| i.time);
        let copied = ui
            .data(|d| d.get_temp::<f64>(id))
            .is_some_and(|at| now - at < COPIED_FOR);
        let font = egui::FontId::monospace(12.5);
        // Room for the value, keeping space for the button beside it.
        let room = (ui.available_width() - 70.0).max(60.0);
        let char_width = ui.fonts_mut(|f| f.glyph_width(&font, '0')).max(1.0);
        let shown = shorten_middle(short, (room / char_width) as usize);
        let value = ui
            .add(
                egui::Label::new(RichText::new(shown).font(font).color(p.text))
                    .sense(egui::Sense::click()),
            )
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text(format!("{full}\n\nClick to copy"));
        let mut copy = value.clicked();
        if copied {
            ui.label(RichText::new("Copied").size(12.0).color(p.success));
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_secs_f64(COPIED_FOR));
        } else {
            copy |= ui
                .small_button("Copy")
                .on_hover_text("Copy to clipboard")
                .clicked();
        }
        if copy {
            ui.ctx().copy_text(full.to_owned());
            ui.data_mut(|d| d.insert_temp(id, now));
        }
    });
    ui.end_row();
}

/// Keeps the start and end of `text` and puts "…" in the middle, so it's
/// at most `max` characters. IDs and paths differ at both ends.
pub fn shorten_middle(text: &str, max: usize) -> String {
    let count = text.chars().count();
    if count <= max || max < 5 {
        return text.to_owned();
    }
    let keep = max - 1;
    let head = keep.div_ceil(2);
    let tail = keep - head;
    let start: String = text.chars().take(head).collect();
    let end: String = text.chars().skip(count - tail).collect();
    format!("{start}…{end}")
}

/// A row of values that link somewhere, e.g. the containers using a
/// volume. Returns the index of the one clicked.
pub fn link_field(ui: &mut egui::Ui, p: &Palette, label: &str, items: &[&str]) -> Option<usize> {
    ui.label(RichText::new(label).color(p.muted));
    let mut clicked = None;
    ui.vertical(|ui| {
        if items.is_empty() {
            ui.label(RichText::new("—").color(p.text));
        }
        for (i, item) in items.iter().enumerate() {
            let link = ui
                .add(
                    egui::Label::new(RichText::new(*item).color(p.accent))
                        .sense(egui::Sense::click()),
                )
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .on_hover_text("Show it");
            if link.clicked() {
                clicked = Some(i);
            }
        }
    });
    ui.end_row();
    clicked
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortens_in_the_middle() {
        assert_eq!(shorten_middle("short", 10), "short");
        assert_eq!(shorten_middle("sha256:abcdef0123456789", 11), "sha25…56789");
        assert_eq!(
            shorten_middle("sha256:abcdef0123456789", 11)
                .chars()
                .count(),
            11
        );
        // Too little room to be useful: leave it whole.
        assert_eq!(shorten_middle("abcdefgh", 3), "abcdefgh");
    }
}
