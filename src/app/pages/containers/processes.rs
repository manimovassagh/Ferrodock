//! Processes tab: what runs inside the container, refreshed while open.

use std::time::Duration;

use eframe::egui::{self, CornerRadius, RichText, Stroke};

use crate::app::page::{Intent, PageCtx};
use crate::docker::{ContainerInfo, plural};

const REFRESH: Duration = Duration::from_secs(2);

/// Columns worth showing, in order, by the names `ps` gives them.
const COLUMNS: [&[&str]; 4] = [&["PID"], &["USER", "UID"], &["TIME"], &["CMD", "COMMAND"]];

pub(super) fn show(ui: &mut egui::Ui, ctx: &mut PageCtx, c: &ContainerInfo) {
    let p = ctx.palette;
    if !c.is_running() {
        ui.add_space(20.0);
        ui.label(RichText::new("Start the container to see its processes.").color(p.muted));
        return;
    }
    let current = ctx
        .store
        .processes
        .as_ref()
        .filter(|(id, _, _)| *id == c.id);
    if current.is_none_or(|(_, at, _)| at.elapsed() >= REFRESH) {
        ctx.request(Intent::Processes(c.id.clone()));
    }
    ui.ctx().request_repaint_after(REFRESH);

    let Some((_, _, Some(result))) = current else {
        ui.add_space(20.0);
        ui.vertical_centered(|ui| ui.spinner());
        return;
    };
    let table = match result {
        Ok(table) => table,
        Err(message) => {
            ui.label(RichText::new(message).color(p.danger));
            return;
        }
    };

    // Pick the familiar columns when `ps` reports them, else show all.
    let picked: Vec<usize> = COLUMNS
        .iter()
        .filter_map(|names| {
            table
                .titles
                .iter()
                .position(|t| names.contains(&t.as_str()))
        })
        .collect();
    let columns = if picked.len() >= 2 {
        picked
    } else {
        (0..table.titles.len()).collect()
    };

    ui.label(
        RichText::new(plural(table.rows.len(), "process"))
            .size(12.0)
            .color(p.muted),
    );
    ui.add_space(4.0);
    egui::Frame::new()
        .fill(p.bg)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(egui::Margin::same(10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| {
                egui::Grid::new("processes")
                    .num_columns(columns.len())
                    .spacing([16.0, 6.0])
                    .striped(true)
                    .show(ui, |ui| {
                        for &i in &columns {
                            ui.label(RichText::new(&table.titles[i]).size(11.0).color(p.muted));
                        }
                        ui.end_row();
                        for row in &table.rows {
                            for &i in &columns {
                                let cell = row.get(i).map(String::as_str).unwrap_or_default();
                                ui.label(RichText::new(cell).monospace().size(12.0).color(p.text));
                            }
                            ui.end_row();
                        }
                    });
            });
        });
}
