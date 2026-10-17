//! Small, generic UI building blocks. Nothing in here knows about Docker.

use eframe::egui::{self, Align2, Color32, CornerRadius, FontId, Response, RichText, Sense, vec2};

use crate::theme::Palette;

/// The Ferrodock mark: a steel hex nut holding a container, on a rust tile.
/// The small-size drawing of the app icon (see assets/source/make_icon.py),
/// painted with shapes so it stays sharp at any scale.
pub fn logo(ui: &mut egui::Ui, size: f32) {
    const RUST: Color32 = Color32::from_rgb(0xd8, 0x50, 0x30);
    const STEEL: Color32 = Color32::from_rgb(0x17, 0x19, 0x1f);
    const CREAM: Color32 = Color32::from_rgb(0xf7, 0xf0, 0xe8);

    let (rect, _) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same((size * 0.22) as u8), RUST);

    // Nut, with a corner pointing up.
    let center = rect.center();
    let r = size * 0.4;
    let nut: Vec<egui::Pos2> = (0..6)
        .map(|i| {
            let angle = (60.0 * i as f32 - 90.0).to_radians();
            center + vec2(angle.cos(), angle.sin()) * r
        })
        .collect();
    painter.add(egui::Shape::convex_polygon(nut, STEEL, egui::Stroke::NONE));

    // Container.
    let container = egui::Rect::from_center_size(center, vec2(r * 1.14, r * 0.66));
    painter.rect_filled(container, CornerRadius::same(1), CREAM);
}

pub fn dot(ui: &mut egui::Ui, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(vec2(8.0, 8.0), Sense::hover());
    ui.painter().circle_filled(rect.center(), 4.0, color);
}

/// A full-width sidebar entry with a count on the right.
pub fn nav_item(
    ui: &mut egui::Ui,
    p: &Palette,
    label: &str,
    count: usize,
    selected: bool,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::click());
    let painter = ui.painter();
    if selected {
        painter.rect_filled(rect, CornerRadius::same(6), p.accent.gamma_multiply(0.18));
    } else if response.hovered() {
        painter.rect_filled(rect, CornerRadius::same(6), p.raised);
    }
    painter.text(
        rect.left_center() + vec2(10.0, 0.0),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(14.0),
        if selected { p.text } else { p.muted },
    );
    painter.text(
        rect.right_center() - vec2(10.0, 0.0),
        Align2::RIGHT_CENTER,
        count.to_string(),
        FontId::proportional(12.0),
        p.muted,
    );
    response
}

/// A tab, optionally with a count, e.g. "Running 3".
pub fn tab(
    ui: &mut egui::Ui,
    p: &Palette,
    label: &str,
    count: Option<usize>,
    selected: bool,
) -> Response {
    let text = match count {
        Some(count) => format!("{label}  {count}"),
        None => label.to_owned(),
    };
    let galley = ui
        .painter()
        .layout_no_wrap(text, FontId::proportional(13.0), p.text);
    let (rect, response) = ui.allocate_exact_size(galley.size() + vec2(24.0, 12.0), Sense::click());
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

/// Rounded "● Label" badge in the given colour.
pub fn pill(ui: &mut egui::Ui, label: &str, color: Color32) {
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_owned(), FontId::proportional(12.0), color);
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

/// A button for destructive actions, tinted with the danger colour.
pub fn danger_button(ui: &mut egui::Ui, p: &Palette, label: &str) -> Response {
    ui.add(
        egui::Button::new(RichText::new(label).color(p.danger)).fill(p.danger.gamma_multiply(0.15)),
    )
}

/// Centered hint shown when a list has nothing to show.
pub fn empty_state(ui: &mut egui::Ui, p: &Palette, message: &str) {
    ui.add_space(60.0);
    ui.vertical_centered(|ui| {
        ui.label(RichText::new(message).color(p.muted));
    });
}

/// "Mounts" style label where an empty list reads as a dash.
pub fn join_or_dash(items: &[impl AsRef<str>]) -> String {
    if items.is_empty() {
        "—".into()
    } else {
        items
            .iter()
            .map(AsRef::as_ref)
            .collect::<Vec<_>>()
            .join("\n")
    }
}
