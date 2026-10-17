//! A small filled line chart for recent values.

use eframe::egui::{self, Color32, CornerRadius, Sense, Shape, Stroke, pos2, vec2};

use crate::theme::Palette;

const HEIGHT: f32 = 56.0;

/// Draws `values` (oldest first) scaled to `0..=max`, full width, with
/// `scale` (what the top of the chart stands for) in the corner.
pub fn show(ui: &mut egui::Ui, p: &Palette, values: &[f64], max: f64, color: Color32, scale: &str) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), HEIGHT), Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect(
        rect,
        CornerRadius::same(6),
        p.bg,
        Stroke::new(1.0, p.border),
        egui::StrokeKind::Inside,
    );
    painter.text(
        rect.right_top() + vec2(-6.0, 4.0),
        egui::Align2::RIGHT_TOP,
        scale,
        egui::FontId::proportional(10.5),
        p.muted,
    );
    if values.len() < 2 || max <= 0.0 {
        return;
    }

    let inner = rect.shrink(4.0);
    let step = inner.width() / (values.len() - 1) as f32;
    let points: Vec<egui::Pos2> = values
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let t = (v / max).clamp(0.0, 1.0) as f32;
            pos2(
                inner.left() + i as f32 * step,
                inner.bottom() - t * inner.height(),
            )
        })
        .collect();

    // Soft fill under the line, drawn as thin vertical bars so it works for
    // any (non-convex) shape.
    let fill = color.gamma_multiply(0.18);
    for pair in points.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        painter.add(Shape::convex_polygon(
            vec![a, b, pos2(b.x, inner.bottom()), pos2(a.x, inner.bottom())],
            fill,
            Stroke::NONE,
        ));
    }
    painter.add(Shape::line(points, Stroke::new(1.5, color)));
}

/// An inline trend line of `size`, without a frame, for table cells.
pub fn tiny(ui: &mut egui::Ui, values: &[f64], max: f64, color: Color32, size: egui::Vec2) {
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    if values.len() < 2 || max <= 0.0 {
        return;
    }
    let step = rect.width() / (values.len() - 1) as f32;
    let points: Vec<egui::Pos2> = values
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let t = (v / max).clamp(0.0, 1.0) as f32;
            pos2(
                rect.left() + i as f32 * step,
                rect.bottom() - t * rect.height(),
            )
        })
        .collect();
    ui.painter()
        .add(Shape::line(points, Stroke::new(1.3, color)));
}
