//! The list table every page uses: header row, whole-row hover and click
//! to select, plus a few standard cell styles.

use eframe::egui::{self, Align, Layout, RichText, Sense};
use egui_extras::{Column, TableBuilder, TableRow};

use super::icons::{self, Icon};
use crate::theme::Palette;

const ROW_HEIGHT: f32 = 48.0;
const COMPACT_ROW_HEIGHT: f32 = 30.0;

/// Switches every table between two-line and one-line rows.
pub fn set_compact(ctx: &egui::Context, compact: bool) {
    ctx.data_mut(|d| d.insert_temp(compact_id(), compact));
}

/// Whether tables use one-line rows (the Compact density setting).
pub fn is_compact(ui: &egui::Ui) -> bool {
    ui.data(|d| d.get_temp(compact_id()).unwrap_or(false))
}

fn compact_id() -> egui::Id {
    egui::Id::unique("ferrodock-compact")
}

/// How many of the shown rows are ticked, for the Select all box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ticked {
    None,
    Some,
    All,
}

impl Ticked {
    pub fn of(ticked: usize, shown: usize) -> Self {
        match ticked {
            0 => Ticked::None,
            n if n >= shown => Ticked::All,
            _ => Ticked::Some,
        }
    }
}

/// A column: header title and sizing.
pub struct Col {
    pub title: &'static str,
    pub width: Column,
    /// For the tick-box column: draws Select all in the header.
    pub ticked: Option<Ticked>,
}

impl Col {
    /// The narrow first column of tick boxes, with Select all on top.
    pub fn ticks(ticked: Ticked) -> Self {
        Self {
            title: "",
            width: Column::exact(28.0),
            ticked: Some(ticked),
        }
    }

    /// Takes the space left over by the fixed columns.
    pub fn fill(title: &'static str) -> Self {
        Self {
            title,
            width: Column::remainder().at_least(160.0).clip(true),
            ticked: None,
        }
    }

    pub fn fixed(title: &'static str, width: f32) -> Self {
        Self {
            title,
            width: Column::exact(width).clip(true),
            ticked: None,
        }
    }
}

/// Asks the next table drawn to move its selection by `delta` rows (↑/↓),
/// or with 0 just to scroll the selected row into view.
pub fn request_step(ctx: &egui::Context, delta: i32) {
    ctx.data_mut(|d| d.insert_temp(step_id(), delta));
}

/// Drops a step no table picked up this frame (e.g. the list was empty).
pub fn clear_step(ctx: &egui::Context) {
    ctx.data_mut(|d| d.remove_temp::<i32>(step_id()));
}

fn step_id() -> egui::Id {
    egui::Id::unique("ferrodock-table-step")
}

fn select_all_id(table: &str) -> egui::Id {
    egui::Id::unique("ferrodock-table-select-all").with(table)
}

/// Whether the Select all box of table `id` was clicked when it was last
/// drawn (this frame).
pub fn select_all_clicked(ui: &egui::Ui, id: &str) -> bool {
    ui.data_mut(|d| d.remove_temp::<bool>(select_all_id(id)))
        .unwrap_or(false)
}

/// A tick box for one row; `name` is what it selects, for screen readers.
/// Returns true when clicked.
pub fn tick(ui: &mut egui::Ui, p: &Palette, on: bool, name: &str) -> bool {
    let state = if on { Ticked::All } else { Ticked::None };
    tick_box(ui, p, state, &format!("Select {name}"))
}

/// A tick box that can also show "some ticked", e.g. for a group.
pub fn tick_state(ui: &mut egui::Ui, p: &Palette, state: Ticked, label: &str) -> bool {
    tick_box(ui, p, state, label)
}

fn tick_box(ui: &mut egui::Ui, p: &Palette, state: Ticked, label: &str) -> bool {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(16.0, 16.0), Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::Role::CheckBox, true, state == Ticked::All, label)
    });
    let painter = ui.painter();
    let corner = egui::CornerRadius::same(4);
    if state == Ticked::None {
        let stroke = if response.hovered() {
            p.accent
        } else {
            p.muted.gamma_multiply(0.6)
        };
        painter.rect_stroke(
            rect.shrink(0.5),
            corner,
            egui::Stroke::new(1.3, stroke),
            egui::StrokeKind::Inside,
        );
    } else {
        painter.rect_filled(rect, corner, p.accent);
        let white = egui::Stroke::new(2.0, p.on_accent);
        let c = rect.center();
        if state == Ticked::All {
            painter.line(
                vec![
                    c + egui::vec2(-4.0, 0.0),
                    c + egui::vec2(-1.0, 3.0),
                    c + egui::vec2(4.5, -3.5),
                ],
                white,
            );
        } else {
            painter.line_segment([c + egui::vec2(-4.0, 0.0), c + egui::vec2(4.0, 0.0)], white);
        }
    }
    response.clicked()
}

/// The row a step of `delta` lands on, if it moves at all.
fn step(current: Option<usize>, delta: i32, rows: usize) -> Option<usize> {
    if rows == 0 || delta == 0 {
        return None;
    }
    let Some(current) = current else {
        return Some(if delta > 0 { 0 } else { rows - 1 });
    };
    let next = (current as i64 + i64::from(delta)).clamp(0, rows as i64 - 1) as usize;
    (next != current).then_some(next)
}

/// Draws a table with `rows` rows. `add_row` fills in the cells of one row
/// (one `row.col(..)` per column). Returns the index of a row that was
/// clicked or stepped to with the keyboard.
pub fn show(
    ui: &mut egui::Ui,
    p: &Palette,
    id: &str,
    columns: Vec<Col>,
    rows: usize,
    is_selected: impl Fn(usize) -> bool,
    mut add_row: impl FnMut(usize, &mut TableRow<'_, '_>),
) -> Option<usize> {
    let row_height = if is_compact(ui) {
        COMPACT_ROW_HEIGHT
    } else {
        ROW_HEIGHT
    };
    let requested = ui.ctx().data_mut(|d| d.remove_temp::<i32>(step_id()));
    let current = requested.and_then(|_| (0..rows).find(|&i| is_selected(i)));
    let stepped = requested.and_then(|delta| step(current, delta, rows));

    let mut clicked = stepped;
    let mut builder = TableBuilder::new(ui)
        .id_salt(id)
        .sense(Sense::click())
        .cell_layout(Layout::left_to_right(Align::Center));
    if let Some(row) = stepped.or(current) {
        builder = builder.scroll_to_row(row, None);
    }
    for col in &columns {
        builder = builder.column(col.width);
    }
    builder
        .header(26.0, |mut header| {
            for col in &columns {
                header.col(|ui| match col.ticked {
                    Some(state) => {
                        let label = if state == Ticked::All {
                            "Select none"
                        } else {
                            "Select all"
                        };
                        if tick_box(ui, p, state, label) {
                            ui.data_mut(|d| d.insert_temp(select_all_id(id), true));
                        }
                    }
                    None => {
                        ui.label(RichText::new(col.title).size(11.0).color(p.muted));
                    }
                });
            }
        })
        .body(|body| {
            body.rows(row_height, rows, |mut row| {
                let index = row.index();
                row.set_selected(is_selected(index));
                add_row(index, &mut row);
                if row.response().clicked() {
                    clicked = Some(index);
                }
            });
        });
    clicked
}

/// Like [`two_line`], with a coloured icon in front, e.g. a container
/// coloured by its state.
pub fn with_icon(
    ui: &mut egui::Ui,
    p: &Palette,
    icon: Icon,
    color: egui::Color32,
    primary: &str,
    secondary: &str,
) {
    let size = if is_compact(ui) { 18.0 } else { 30.0 };
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), Sense::hover());
    ui.painter().rect_filled(
        rect,
        egui::CornerRadius::same(7),
        color.gamma_multiply(0.12),
    );
    icons::paint(ui.painter(), rect.shrink(size * 0.22), icon, color);
    ui.add_space(4.0);
    two_line(ui, p, primary, secondary);
}

/// Primary text with a muted second line, e.g. name over image. In
/// compact mode both go on one line.
pub fn two_line(ui: &mut egui::Ui, p: &Palette, primary: &str, secondary: &str) {
    let primary_text = RichText::new(primary)
        .font(egui::FontId::new(13.5, crate::theme::semibold()))
        .color(p.text);
    if is_compact(ui) {
        label(ui, primary_text);
        label(ui, RichText::new(secondary).size(12.0).color(p.muted));
        return;
    }
    ui.vertical(|ui| {
        ui.add_space(6.0);
        label(ui, primary_text);
        label(ui, RichText::new(secondary).size(12.0).color(p.muted));
    });
}

/// Whether the pointer is over the row this cell belongs to.
pub fn row_hovered(ui: &egui::Ui) -> bool {
    let cell = ui.max_rect();
    ui.ctx().pointer_hover_pos().is_some_and(|pos| {
        ui.clip_rect().contains(pos) && pos.y >= cell.top() && pos.y < cell.bottom()
    })
}

pub fn text(ui: &mut egui::Ui, p: &Palette, value: &str) {
    label(ui, RichText::new(value).color(p.text));
}

pub fn muted(ui: &mut egui::Ui, p: &Palette, value: &str) {
    label(ui, RichText::new(value).color(p.muted));
}

pub fn mono(ui: &mut egui::Ui, p: &Palette, value: &str) {
    label(ui, RichText::new(value).monospace().color(p.muted));
}

/// Non-selectable, truncating label so clicks select the row instead.
fn label(ui: &mut egui::Ui, text: RichText) {
    ui.add(egui::Label::new(text).truncate().selectable(false));
}

#[cfg(test)]
mod tests {
    use super::step;

    #[test]
    fn steps_through_rows() {
        assert_eq!(step(None, 1, 3), Some(0));
        assert_eq!(step(None, -1, 3), Some(2));
        assert_eq!(step(Some(0), 1, 3), Some(1));
        assert_eq!(step(Some(2), 1, 3), None);
        assert_eq!(step(Some(0), -1, 3), None);
        assert_eq!(step(Some(1), 0, 3), None);
        assert_eq!(step(None, 1, 0), None);
    }
}
