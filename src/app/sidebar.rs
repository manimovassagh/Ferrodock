//! Left sidebar: brand, page navigation and Docker engine status.

use eframe::egui::{self, Align, Layout, RichText};

use super::components::widgets;
use super::page::Page;
use super::store::{Connection, Store};
use crate::theme::Palette;

/// Draws the sidebar. Returns the index of a page the user clicked.
pub fn show(
    ui: &mut egui::Ui,
    p: &Palette,
    pages: &[Box<dyn Page>],
    active: usize,
    store: &Store,
) -> Option<usize> {
    ui.add_space(2.0);
    ui.horizontal(|ui| {
        widgets::logo(ui, 24.0);
        ui.label(RichText::new("Ferrodock").size(17.0).strong().color(p.text));
    });

    ui.add_space(22.0);
    ui.label(RichText::new("DOCKER").size(11.0).color(p.muted));
    ui.add_space(2.0);
    let mut clicked = None;
    for (i, page) in pages.iter().enumerate() {
        if widgets::nav_item(ui, p, page.title(), &page.badge(store), i == active).clicked() {
            clicked = Some(i);
        }
    }

    ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
        ui.horizontal(|ui| engine_status(ui, p, &store.connection));
    });
    clicked
}

fn engine_status(ui: &mut egui::Ui, p: &Palette, connection: &Connection) {
    match connection {
        Connection::Connecting => {
            ui.spinner();
            ui.label(RichText::new("Connecting…").color(p.muted));
        }
        Connection::Connected { version } => {
            widgets::dot(ui, p.success);
            ui.label(RichText::new(format!("Docker {version}")).color(p.muted));
        }
        Connection::Disconnected { .. } => {
            widgets::dot(ui, p.danger);
            ui.label(RichText::new("Docker not running").color(p.muted));
        }
    }
}
