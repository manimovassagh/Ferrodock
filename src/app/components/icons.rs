//! Line icons drawn with shapes, in the style of Lucide (24×24 grid, round
//! caps). They're crisp at any size and don't depend on which glyphs a
//! font happens to have.

use eframe::egui::{self, Color32, Pos2, Rect, Response, Sense, Shape, Stroke, vec2};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Play,
    Stop,
    Pause,
    Restart,
    Refresh,
    Close,
    Gear,
    Search,
    Download,
    // Pages.
    Container,
    Stack,
    Layers,
    Database,
    Network,
    HardDrive,
}

/// Paints `icon` filling `rect` (drawn on a 24-unit grid).
pub fn paint(painter: &egui::Painter, rect: Rect, icon: Icon, color: Color32) {
    let s = rect.width().min(rect.height()) / 24.0;
    let o = rect.center() - vec2(12.0, 12.0) * s;
    let p = |x: f32, y: f32| -> Pos2 { o + vec2(x, y) * s };
    let stroke = Stroke::new((1.8 * s).max(1.1), color);
    let line = |pts: &[Pos2]| Shape::line(pts.to_vec(), stroke);
    let poly = |pts: &[Pos2]| Shape::closed_line(pts.to_vec(), stroke);
    let fill = |pts: &[Pos2]| Shape::convex_polygon(pts.to_vec(), color, Stroke::NONE);
    let circle = |c: Pos2, r: f32| Shape::circle_stroke(c, r * s, stroke);
    let arc = |c: Pos2, r: f32, from: f32, to: f32| {
        let steps = 24;
        let pts: Vec<Pos2> = (0..=steps)
            .map(|i| {
                let a = (from + (to - from) * i as f32 / steps as f32).to_radians();
                c + vec2(a.cos(), a.sin()) * r * s
            })
            .collect();
        Shape::line(pts, stroke)
    };

    let shapes: Vec<Shape> = match icon {
        Icon::Play => vec![fill(&[p(7.0, 4.5), p(19.5, 12.0), p(7.0, 19.5)])],
        Icon::Stop => vec![Shape::rect_filled(
            Rect::from_min_max(p(6.0, 6.0), p(18.0, 18.0)),
            2.0 * s,
            color,
        )],
        Icon::Pause => vec![
            Shape::rect_filled(
                Rect::from_min_max(p(6.5, 5.0), p(10.0, 19.0)),
                1.2 * s,
                color,
            ),
            Shape::rect_filled(
                Rect::from_min_max(p(14.0, 5.0), p(17.5, 19.0)),
                1.2 * s,
                color,
            ),
        ],
        Icon::Restart => vec![
            arc(p(12.0, 12.0), 7.5, -60.0, 230.0),
            line(&[p(3.5, 4.5), p(5.5, 9.8), p(10.5, 8.0)]),
        ],
        Icon::Refresh => vec![
            arc(p(12.0, 12.0), 7.5, 200.0, 340.0),
            line(&[p(19.8, 4.5), p(19.4, 9.6), p(14.5, 8.8)]),
            arc(p(12.0, 12.0), 7.5, 20.0, 160.0),
            line(&[p(4.2, 19.5), p(4.6, 14.4), p(9.5, 15.2)]),
        ],
        Icon::Close => vec![
            line(&[p(6.0, 6.0), p(18.0, 18.0)]),
            line(&[p(18.0, 6.0), p(6.0, 18.0)]),
        ],
        Icon::Gear => {
            let mut v = vec![circle(p(12.0, 12.0), 3.0), circle(p(12.0, 12.0), 7.0)];
            for i in 0..8 {
                let a = (i as f32 * 45.0).to_radians();
                let d = vec2(a.cos(), a.sin());
                v.push(line(&[
                    p(12.0, 12.0) + d * 7.0 * s,
                    p(12.0, 12.0) + d * 9.8 * s,
                ]));
            }
            v
        }
        Icon::Download => vec![
            line(&[p(12.0, 3.5), p(12.0, 15.0)]),
            line(&[p(7.0, 10.0), p(12.0, 15.0), p(17.0, 10.0)]),
            line(&[p(4.0, 15.5), p(4.0, 20.0), p(20.0, 20.0), p(20.0, 15.5)]),
        ],
        Icon::Search => vec![
            circle(p(11.0, 11.0), 6.5),
            line(&[p(16.0, 16.0), p(20.5, 20.5)]),
        ],
        Icon::Container => vec![
            poly(&[
                p(12.0, 3.0),
                p(20.0, 7.5),
                p(20.0, 16.5),
                p(12.0, 21.0),
                p(4.0, 16.5),
                p(4.0, 7.5),
            ]),
            line(&[p(4.0, 7.5), p(12.0, 12.0), p(20.0, 7.5)]),
            line(&[p(12.0, 12.0), p(12.0, 21.0)]),
        ],
        Icon::Stack => vec![
            poly(&[p(12.0, 3.0), p(21.0, 7.5), p(12.0, 12.0), p(3.0, 7.5)]),
            line(&[p(3.0, 12.0), p(12.0, 16.5), p(21.0, 12.0)]),
            line(&[p(3.0, 16.5), p(12.0, 21.0), p(21.0, 16.5)]),
        ],
        Icon::Layers => vec![
            Shape::rect_stroke(
                Rect::from_min_max(p(3.5, 3.5), p(20.5, 20.5)),
                3.0 * s,
                stroke,
                egui::StrokeKind::Middle,
            ),
            circle(p(12.0, 12.0), 4.0),
            Shape::circle_filled(p(12.0, 12.0), 1.2 * s, color),
        ],
        Icon::Database => vec![
            arc(p(12.0, 6.0), 8.0, 0.0, 360.0).scaled_y(p(12.0, 6.0), 0.38),
            line(&[p(4.0, 6.0), p(4.0, 18.0)]),
            line(&[p(20.0, 6.0), p(20.0, 18.0)]),
            arc(p(12.0, 12.0), 8.0, 0.0, 180.0).scaled_y(p(12.0, 12.0), 0.38),
            arc(p(12.0, 18.0), 8.0, 0.0, 180.0).scaled_y(p(12.0, 18.0), 0.38),
        ],
        Icon::Network => vec![
            Shape::rect_stroke(
                Rect::from_min_max(p(9.0, 2.5), p(15.0, 8.0)),
                1.5 * s,
                stroke,
                egui::StrokeKind::Middle,
            ),
            Shape::rect_stroke(
                Rect::from_min_max(p(2.5, 16.0), p(8.5, 21.5)),
                1.5 * s,
                stroke,
                egui::StrokeKind::Middle,
            ),
            Shape::rect_stroke(
                Rect::from_min_max(p(15.5, 16.0), p(21.5, 21.5)),
                1.5 * s,
                stroke,
                egui::StrokeKind::Middle,
            ),
            line(&[p(12.0, 8.0), p(12.0, 12.0)]),
            line(&[p(5.5, 16.0), p(5.5, 12.0), p(18.5, 12.0), p(18.5, 16.0)]),
        ],
        Icon::HardDrive => vec![
            Shape::rect_stroke(
                Rect::from_min_max(p(3.0, 12.5), p(21.0, 19.5)),
                2.0 * s,
                stroke,
                egui::StrokeKind::Middle,
            ),
            line(&[p(3.0, 12.5), p(6.5, 5.0), p(17.5, 5.0), p(21.0, 12.5)]),
            Shape::circle_filled(p(16.5, 16.0), 1.1 * s, color),
            Shape::circle_filled(p(13.0, 16.0), 1.1 * s, color),
        ],
    };
    painter.extend(shapes);
}

/// Allocates a `size`×`size` square and paints `icon` in it.
pub fn show(ui: &mut egui::Ui, icon: Icon, size: f32, color: Color32) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
    paint(ui.painter(), rect, icon, color);
    response
}

trait ScaledY {
    fn scaled_y(self, center: Pos2, factor: f32) -> Shape;
}

impl ScaledY for Shape {
    /// Squashes a line vertically around `center`, turning a circle into
    /// an ellipse (for the database icon).
    fn scaled_y(self, center: Pos2, factor: f32) -> Shape {
        match self {
            Shape::Path(mut path) => {
                for point in &mut path.points {
                    point.y = center.y + (point.y - center.y) * factor;
                }
                Shape::Path(path)
            }
            other => other,
        }
    }
}
