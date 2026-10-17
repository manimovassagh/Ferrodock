//! Small reusable pieces of IronStack's UI.

use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Response, RichText, Sense, Stroke, vec2,
};

use crate::docker::{Action, ContainerInfo};
use crate::theme::{self, Palette};

/// The IronStack mark: three stacked slabs.
pub fn logo(ui: &mut egui::Ui, p: &Palette, size: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
    let painter = ui.painter();
    let slab = size * 0.24;
    let gap = (size - 3.0 * slab) / 2.0;
    for (i, alpha) in [1.0, 0.7, 0.45].into_iter().enumerate() {
        let top = rect.top() + i as f32 * (slab + gap);
        let inset = i as f32 * size * 0.08;
        let slab_rect = egui::Rect::from_min_max(
            egui::pos2(rect.left() + inset, top),
            egui::pos2(rect.right() - inset, top + slab),
        );
        painter.rect_filled(
            slab_rect,
            CornerRadius::same(2),
            p.accent.gamma_multiply(alpha),
        );
    }
}

pub fn dot(ui: &mut egui::Ui, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(vec2(8.0, 8.0), Sense::hover());
    ui.painter().circle_filled(rect.center(), 4.0, color);
}

/// A full-width sidebar entry with an optional count on the right.
pub fn nav_item(
    ui: &mut egui::Ui,
    p: &Palette,
    label: &str,
    count: Option<usize>,
    selected: bool,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::click());
    let painter = ui.painter();
    if selected {
        painter.rect_filled(rect, CornerRadius::same(6), p.accent.gamma_multiply(0.18));
    } else if response.hovered() {
        painter.rect_filled(rect, CornerRadius::same(6), p.raised);
    }
    let text_color = if selected { p.text } else { p.muted };
    painter.text(
        rect.left_center() + vec2(10.0, 0.0),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(14.0),
        text_color,
    );
    if let Some(count) = count {
        painter.text(
            rect.right_center() - vec2(10.0, 0.0),
            Align2::RIGHT_CENTER,
            count.to_string(),
            FontId::proportional(12.0),
            p.muted,
        );
    }
    response
}

/// A tab with a count, used for All / Running / Stopped.
pub fn tab(ui: &mut egui::Ui, p: &Palette, label: &str, count: usize, selected: bool) -> Response {
    let text = format!("{label}  {count}");
    let galley = ui
        .painter()
        .layout_no_wrap(text, FontId::proportional(13.0), p.text);
    let size = galley.size() + vec2(24.0, 12.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let painter = ui.painter();
    if selected {
        painter.rect_filled(rect, CornerRadius::same(6), p.raised);
    } else if response.hovered() {
        painter.rect_filled(rect, CornerRadius::same(6), p.raised.gamma_multiply(0.5));
    }
    let color = if selected { p.text } else { p.muted };
    painter.galley_with_override_text_color(rect.min + vec2(12.0, 6.0), galley, color);
    response
}

/// Rounded "● Running" style badge for a container state.
pub fn status_pill(ui: &mut egui::Ui, p: &Palette, state: &str) {
    let color = theme::state_color(p, state);
    let label = capitalize(state);
    let galley = ui
        .painter()
        .layout_no_wrap(label, FontId::proportional(12.0), color);
    let padding = vec2(8.0, 3.0);
    let dot_space = 12.0;
    let size = galley.size() + padding * 2.0 + vec2(dot_space, 0.0);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(99), color.gamma_multiply(0.15));
    painter.circle_filled(
        egui::pos2(rect.left() + padding.x + 3.0, rect.center().y),
        3.0,
        color,
    );
    painter.galley(rect.min + padding + vec2(dot_space, 0.0), galley, color);
}

/// Compact square button with a glyph and a tooltip.
pub fn icon_button(ui: &mut egui::Ui, icon: &str, tooltip: &str) -> Response {
    ui.add(egui::Button::new(RichText::new(icon).size(14.0)).min_size(vec2(28.0, 26.0)))
        .on_hover_text(tooltip)
}

/// Start/Stop and Restart controls for a container. Shows a spinner while
/// an action is in flight. Returns the clicked action, if any.
pub fn container_actions(
    ui: &mut egui::Ui,
    c: &ContainerInfo,
    busy: bool,
    with_labels: bool,
) -> Option<Action> {
    if busy {
        ui.spinner();
        return None;
    }
    let toggle = if c.is_running() || c.is_paused() {
        (Action::Stop, "■", "Stop")
    } else {
        (Action::Start, "▶", "Start")
    };
    let mut clicked = None;
    for (action, icon, label) in [toggle, (Action::Restart, "⟳", "Restart")] {
        let response = if with_labels {
            ui.button(format!("{icon}  {label}"))
        } else {
            icon_button(ui, icon, label)
        };
        if response.clicked() {
            clicked = Some(action);
        }
    }
    clicked
}

/// A titled card used in the details pane.
pub fn section(
    ui: &mut egui::Ui,
    p: &Palette,
    title: &str,
    add_contents: impl FnOnce(&mut egui::Ui),
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
            add_contents(ui);
        });
    ui.add_space(14.0);
}

/// One label/value line inside a two-column details grid.
pub fn field(ui: &mut egui::Ui, p: &Palette, label: &str, value: &str) {
    ui.label(RichText::new(label).color(p.muted));
    let value = if value.is_empty() { "—" } else { value };
    ui.add(egui::Label::new(RichText::new(value).color(p.text)).wrap());
    ui.end_row();
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}
