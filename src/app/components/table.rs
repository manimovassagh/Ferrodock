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

/// Draws a table with `rows` rows. `add_row` fills in the cells of one row
/// (one `row.col(..)` per column). Returns the index of a clicked row.
pub fn show(
    ui: &mut egui::Ui,
    p: &Palette,
    id: &str,
    columns: Vec<Col>,
    rows: usize,
    is_selected: impl Fn(usize) -> bool,
    mut add_row: impl FnMut(usize, &mut TableRow<'_, '_>),
) -> Option<usize> {
    let mut clicked = None;
    let mut builder = TableBuilder::new(ui)
        .id_salt(id)
        .sense(Sense::click())
        .cell_layout(Layout::left_to_right(Align::Center));
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
