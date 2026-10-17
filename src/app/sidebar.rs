//! Left sidebar: brand, page navigation, the settings button and Docker
//! engine status.

use eframe::egui::{self, Align, Layout, RichText};

use super::components::icons::Icon;
use super::components::widgets;
use super::page::Page;
use super::store::{Connection, Store};
use crate::theme::Palette;

/// What the user clicked in the sidebar.
pub enum Click {
    Page(usize),
    Settings,
}

/// Draws the sidebar.
pub fn show(
    ui: &mut egui::Ui,
    p: &Palette,
    pages: &[Box<dyn Page>],
    active: usize,
    store: &Store,
) -> Option<Click> {
    ui.add_space(2.0);
    ui.horizontal(|ui| {
        widgets::logo(ui, 26.0);
        ui.label(
            RichText::new("Ferrodock")
                .font(egui::FontId::new(16.0, crate::theme::semibold()))
                .color(p.text),
        );
    });

    ui.add_space(22.0);
    ui.label(RichText::new("DOCKER").size(11.0).color(p.muted));
    ui.add_space(2.0);
    let mut clicked = None;
    for (i, page) in pages.iter().enumerate() {
        let item = widgets::nav_item(
            ui,
            p,
            page.icon(),
            page.title(),
            &page.badge(store),
            i == active,
        );
        if item.clicked() {
            clicked = Some(Click::Page(i));
        }
    }

    ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
        ui.horizontal(|ui| engine_status(ui, p, &store.connection));
        ui.add_space(6.0);
        let settings =
            widgets::nav_item(ui, p, Icon::Gear, "Settings", "", false).on_hover_text(format!(
                "Theme, density and interface size ({}{})",
                super::keys::COMMAND,
                ","
            ));
        if settings.clicked() {
            clicked = Some(Click::Settings);
        }
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
