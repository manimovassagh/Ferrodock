//! Small, generic UI building blocks. Nothing in here knows about Docker.

use eframe::egui::{self, Align2, Color32, CornerRadius, FontId, Response, RichText, Sense, vec2};

use super::icons::{self, Icon};
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

/// A full-width sidebar entry: icon, label, and a badge (usually a count)
/// on the right.
pub fn nav_item(
    ui: &mut egui::Ui,
    p: &Palette,
    icon: Icon,
    label: &str,
    badge: &str,
    selected: bool,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::click());
    let painter = ui.painter();
    if selected {
        painter.rect_filled(rect, CornerRadius::same(8), p.accent.gamma_multiply(0.13));
    } else if response.hovered() {
        painter.rect_filled(rect, CornerRadius::same(8), p.raised);
    }
    let icon_rect =
        egui::Rect::from_center_size(rect.left_center() + vec2(19.0, 0.0), vec2(17.0, 17.0));
    icons::paint(
        painter,
        icon_rect,
        icon,
        if selected { p.accent } else { p.muted },
    );
    painter.text(
        rect.left_center() + vec2(36.0, 0.0),
        Align2::LEFT_CENTER,
        label,
        if selected {
            FontId::new(13.5, crate::theme::semibold())
        } else {
            FontId::proportional(13.5)
        },
        p.text,
    );
    painter.text(
        rect.right_center() - vec2(10.0, 0.0),
        Align2::RIGHT_CENTER,
        badge,
        FontId::proportional(12.0),
        p.muted,
    );
    response.widget_info(|| egui::WidgetInfo::selected(egui::Role::Tab, true, selected, label));
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A highlighted sidebar card: an icon, a title and a line under it, on
/// a soft gradient from the accent into the palette's second colour, with
/// a matching hairline border. Brighter on hover.
pub fn feature_card(
    ui: &mut egui::Ui,
    p: &Palette,
    icon: Icon,
    title: &str,
    subtitle: &str,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 46.0), Sense::click());
    let painter = ui.painter();
    let (from, to) = (p.accent, p.glow);
    let dark = ui.visuals().dark_mode;
    let strength = match (response.hovered(), dark) {
        (true, true) => 0.24,
        (true, false) => 0.16,
        (false, true) => 0.15,
        (false, false) => 0.09,
    };
    let blend = move |t: f32, alpha: f32| -> Color32 {
        let t = t.clamp(0.0, 1.0);
        let mix = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round() as u8;
        Color32::from_rgb(
            mix(from.r(), to.r()),
            mix(from.g(), to.g()),
            mix(from.b(), to.b()),
        )
        .gamma_multiply(alpha)
    };

    // The fill: a fan over the rounded outline, coloured left to right.
    let mut outline = Vec::new();
    egui::epaint::tessellator::path::rounded_rectangle(
        &mut outline,
        egui::epaint::RoundedRect::new(rect, CornerRadius::same(10)),
    );
    let along = |pos: egui::Pos2| (pos.x - rect.left()) / rect.width().max(1.0);
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(rect.center(), blend(0.5, strength));
    for point in &outline {
        mesh.colored_vertex(*point, blend(along(*point), strength));
    }
    let n = outline.len() as u32;
    for i in 0..n {
        mesh.add_triangle(0, 1 + i, 1 + (i + 1) % n);
    }
    painter.add(mesh);
    painter.add(egui::epaint::PathShape {
        points: outline,
        closed: true,
        fill: Color32::TRANSPARENT,
        stroke: egui::epaint::PathStroke::new_uv(1.0, move |bounds, pos| {
            blend((pos.x - bounds.left()) / bounds.width().max(1.0), 0.55)
        }),
    });

    let icon_rect =
        egui::Rect::from_center_size(rect.left_center() + vec2(20.0, 0.0), vec2(18.0, 18.0));
    icons::paint(painter, icon_rect, icon, p.accent);
    painter.text(
        rect.left_center() + vec2(38.0, -8.0),
        Align2::LEFT_CENTER,
        title,
        FontId::new(13.5, crate::theme::semibold()),
        p.text,
    );
    painter.text(
        rect.left_center() + vec2(38.0, 9.0),
        Align2::LEFT_CENTER,
        subtitle,
        FontId::proportional(11.5),
        p.muted,
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, title));
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A rounded track holding [`tab`]s, like a macOS segmented control.
pub fn segmented<R>(ui: &mut egui::Ui, p: &Palette, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::new()
        .fill(p.raised)
        .corner_radius(CornerRadius::same(9))
        .inner_margin(egui::Margin::same(3))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            ui.horizontal(add).inner
        })
        .inner
}

/// One segment of a [`segmented`] control, optionally with a count, e.g.
/// "Running 3".
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
    let font = if selected {
        FontId::new(13.0, crate::theme::semibold())
    } else {
        FontId::proportional(13.0)
    };
    let galley = ui.painter().layout_no_wrap(text, font, p.text);
    let (rect, response) = ui.allocate_exact_size(galley.size() + vec2(22.0, 10.0), Sense::click());
    let painter = ui.painter();
    if selected {
        painter.add(egui::Shape::rect_filled(
            rect.translate(vec2(0.0, 0.5)),
            CornerRadius::same(7),
            Color32::from_black_alpha(18),
        ));
        painter.rect_filled(rect, CornerRadius::same(7), p.bg);
    } else if response.hovered() {
        painter.rect_filled(rect, CornerRadius::same(7), p.border.gamma_multiply(0.6));
    }
    let color = if selected { p.text } else { p.muted };
    painter.galley_with_override_text_color(rect.min + vec2(11.0, 5.0), galley, color);
    response.widget_info(|| egui::WidgetInfo::selected(egui::Role::Tab, true, selected, label));
    response
}

/// The mark at the start of a [`pill`], so states differ by shape as well
/// as colour.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    Dot,
    Ring,
    Square,
    Triangle,
}

/// Rounded "● Label" badge in the given colour.
pub fn pill(ui: &mut egui::Ui, label: &str, color: Color32) {
    pill_with(ui, label, color, Mark::Dot);
}

/// A [`pill`] with a chosen mark.
pub fn pill_with(ui: &mut egui::Ui, label: &str, color: Color32, mark: Mark) {
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_owned(), FontId::proportional(12.0), color);
    let padding = vec2(8.0, 3.0);
    let dot_space = 12.0;
    let size = galley.size() + padding * 2.0 + vec2(dot_space, 0.0);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(99), color.gamma_multiply(0.15));
    let center = egui::pos2(rect.left() + padding.x + 3.0, rect.center().y);
    match mark {
        Mark::Dot => {
            painter.circle_filled(center, 3.0, color);
        }
        Mark::Ring => {
            painter.circle_stroke(center, 2.6, egui::Stroke::new(1.2, color));
        }
        Mark::Square => {
            let square = egui::Rect::from_center_size(center, vec2(5.5, 5.5));
            painter.rect_filled(square, CornerRadius::same(1), color);
        }
        Mark::Triangle => {
            let points = vec![
                center + vec2(0.0, -3.6),
                center + vec2(3.4, 2.6),
                center + vec2(-3.4, 2.6),
            ];
            painter.add(egui::Shape::convex_polygon(
                points,
                color,
                egui::Stroke::NONE,
            ));
        }
    }
    painter.galley(rect.min + padding + vec2(dot_space, 0.0), galley, color);
}

/// Compact square button showing an icon, with a tooltip.
pub fn icon_button(ui: &mut egui::Ui, icon: Icon, tooltip: &str) -> Response {
    let p = crate::theme::palette(ui);
    let (rect, response) = ui.allocate_exact_size(vec2(28.0, 28.0), Sense::click());
    let painter = ui.painter();
    if response.hovered() {
        painter.rect_filled(rect, CornerRadius::same(7), p.raised);
    }
    let color = if response.hovered() { p.text } else { p.muted };
    icons::paint(
        painter,
        egui::Rect::from_center_size(rect.center(), vec2(15.0, 15.0)),
        icon,
        color,
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, tooltip));
    response
        .on_hover_text(tooltip)
        .on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A regular button with an icon before its label.
pub fn button(ui: &mut egui::Ui, icon: Icon, label: &str) -> Response {
    let p = crate::theme::palette(ui);
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_owned(), FontId::proportional(13.0), p.text);
    let size = vec2(galley.size().x + 44.0, 28.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let painter = ui.painter();
    let fill = if response.is_pointer_button_down_on() {
        p.border
    } else if response.hovered() {
        p.border.gamma_multiply(0.7)
    } else {
        p.raised
    };
    painter.rect_filled(rect, CornerRadius::same(8), fill);
    icons::paint(
        painter,
        egui::Rect::from_center_size(rect.left_center() + vec2(19.0, 0.0), vec2(14.0, 14.0)),
        icon,
        p.text,
    );
    painter.galley(
        egui::pos2(rect.left() + 32.0, rect.center().y - galley.size().y / 2.0),
        galley,
        p.text,
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, label));
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// The main action of a dialog or pane: filled with the accent colour.
pub fn primary_button(ui: &mut egui::Ui, label: &str) -> Response {
    let p = crate::theme::palette(ui);
    let enabled = ui.is_enabled();
    let galley = ui.painter().layout_no_wrap(
        label.to_owned(),
        FontId::new(13.5, crate::theme::semibold()),
        p.on_accent,
    );
    let size = vec2(galley.size().x + 32.0, 30.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let fill = if !enabled {
        p.accent.gamma_multiply(0.4)
    } else if response.is_pointer_button_down_on() {
        p.accent.gamma_multiply(0.85)
    } else if response.hovered() {
        p.accent.gamma_multiply(0.92)
    } else {
        p.accent
    };
    ui.painter().rect_filled(rect, CornerRadius::same(8), fill);
    ui.painter()
        .galley(rect.center() - galley.size() / 2.0, galley, p.on_accent);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, label));
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A button for destructive actions, tinted with the danger colour.
pub fn danger_button(ui: &mut egui::Ui, p: &Palette, label: &str) -> Response {
    ui.add(
        egui::Button::new(RichText::new(label).color(p.danger)).fill(p.danger.gamma_multiply(0.15)),
    )
}

/// Space kept between a dialog and the window's edges.
const DIALOG_MARGIN: f32 = 24.0;

/// A dialog's body, between its title and its buttons, in a scroll area
/// that keeps the whole dialog within the window, with a margin. The body
/// scrolls when the window is too small for it; the title and buttons stay
/// put.
pub fn dialog_body<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let id = ui.scope_id().with("dialog-body");
    let room = ui.ctx().content_rect().height() - 2.0 * DIALOG_MARGIN;
    // Everything around the body (frame, title, buttons) is as tall as it
    // was last frame.
    let dialog = ui.ctx().memory(|m| m.area_rect(ui.layer_id().id));
    let around = match (dialog, ui.data(|d| d.get_temp::<f32>(id))) {
        (Some(dialog), Some(body)) => dialog.height() - body,
        _ => 0.0,
    };
    let output = egui::ScrollArea::vertical()
        .id_salt(id)
        .max_height((room - around).max(60.0))
        .auto_shrink([false, true])
        .show(ui, add);
    ui.data_mut(|d| d.insert_temp(id, output.inner_rect.height()));
    output.inner
}

/// Shown when a list has nothing in it: an icon, a title, a hint and an
/// optional button. Returns true when the button was clicked.
pub fn empty_state(
    ui: &mut egui::Ui,
    p: &Palette,
    icon: Icon,
    title: &str,
    hint: &str,
    action: Option<&str>,
) -> bool {
    let mut clicked = false;
    ui.add_space(56.0);
    ui.vertical_centered(|ui| {
        let (rect, _) = ui.allocate_exact_size(vec2(64.0, 64.0), Sense::hover());
        ui.painter()
            .rect_filled(rect, CornerRadius::same(18), p.accent.gamma_multiply(0.1));
        icons::paint(ui.painter(), rect.shrink(18.0), icon, p.accent);
        ui.add_space(14.0);
        ui.label(
            RichText::new(title)
                .font(FontId::new(17.0, crate::theme::semibold()))
                .color(p.text),
        );
        ui.add_space(4.0);
        ui.label(RichText::new(hint).color(p.muted));
        if let Some(label) = action {
            ui.add_space(14.0);
            clicked = primary_button(ui, label).clicked();
        }
    });
    clicked
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
