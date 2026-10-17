//! The list table every page uses: header row, whole-row hover and click
//! to select, plus a few standard cell styles.

use eframe::egui::{self, Align, Layout, RichText, Sense};
use egui_extras::{Column, TableBuilder, TableRow};

use crate::theme::Palette;

pub const ROW_HEIGHT: f32 = 48.0;

/// A column: header title and sizing.
pub struct Col {
    pub title: &'static str,
    pub width: Column,
}

impl Col {
    /// Takes the space left over by the fixed columns.
    pub fn fill(title: &'static str) -> Self {
        Self {
            title,
            width: Column::remainder().at_least(160.0).clip(true),
        }
    }

    pub fn fixed(title: &'static str, width: f32) -> Self {
        Self {
            title,
            width: Column::exact(width).clip(true),
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
                header.col(|ui| {
                    ui.label(RichText::new(col.title).size(11.0).color(p.muted));
                });
            }
        })
        .body(|body| {
            body.rows(ROW_HEIGHT, rows, |mut row| {
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

/// Bold primary text with a muted second line, e.g. name over image.
pub fn two_line(ui: &mut egui::Ui, p: &Palette, primary: &str, secondary: &str) {
    ui.vertical(|ui| {
        ui.add_space(6.0);
        label(ui, RichText::new(primary).strong().color(p.text));
        label(ui, RichText::new(secondary).size(12.0).color(p.muted));
    });
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
