//! Maps a [`Palette`] onto egui's widget styling.

use eframe::egui::{CornerRadius, Stroke, Visuals};

use super::Palette;

pub fn visuals(mut v: Visuals, p: &Palette) -> Visuals {
    let radius = CornerRadius::same(6);

    v.panel_fill = p.bg;
    v.window_fill = p.sidebar;
    v.window_stroke = Stroke::new(1.0, p.border);
    v.window_corner_radius = CornerRadius::same(8);
    v.extreme_bg_color = p.sidebar;
    v.faint_bg_color = p.sidebar;
    v.hyperlink_color = p.accent;
    v.error_fg_color = p.danger;
    v.warn_fg_color = p.warning;
    v.selection.bg_fill = p.accent.gamma_multiply(0.35);
    v.selection.stroke = Stroke::new(1.0, p.text);

    let w = &mut v.widgets;
    w.noninteractive.bg_stroke = Stroke::new(1.0, p.border);
    w.noninteractive.fg_stroke = Stroke::new(1.0, p.text);
    w.noninteractive.corner_radius = radius;

    w.inactive.weak_bg_fill = p.raised;
    w.inactive.bg_fill = p.raised;
    w.inactive.bg_stroke = Stroke::NONE;
    w.inactive.fg_stroke = Stroke::new(1.0, p.text);
    w.inactive.corner_radius = radius;

    w.hovered.weak_bg_fill = p.border;
    w.hovered.bg_fill = p.border;
    w.hovered.bg_stroke = Stroke::NONE;
    w.hovered.fg_stroke = Stroke::new(1.0, p.text);
    w.hovered.corner_radius = radius;
    w.hovered.expansion = 0.0;

    w.active.weak_bg_fill = p.accent.gamma_multiply(0.5);
    w.active.bg_fill = p.accent.gamma_multiply(0.5);
    w.active.bg_stroke = Stroke::NONE;
    w.active.fg_stroke = Stroke::new(1.0, p.text);
    w.active.corner_radius = radius;
    w.active.expansion = 0.0;

    w.open = w.active;
    v
}
