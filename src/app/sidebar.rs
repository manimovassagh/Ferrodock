//! Left sidebar: brand, page navigation, the settings button and Docker
//! engine status.

use eframe::egui::{self, Align, Layout, RichText};

use super::components::icons::Icon;
use super::components::widgets;
use super::page::Page;
use super::store::{Connection, Store};
use crate::theme::Palette;
use crate::update::Status;

/// What the user clicked in the sidebar.
pub enum Click {
    Page(usize),
    Settings,
    /// Download and install the available update.
    Update,
    /// Restart into the installed update.
    Restart,
}

/// Draws the sidebar.
pub fn show(
    ui: &mut egui::Ui,
    p: &Palette,
    pages: &[Box<dyn Page>],
    active: usize,
    store: &Store,
    update: &Status,
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
        if let Some(click) = update_item(ui, p, update) {
            clicked = Some(click);
        }
    });
    clicked
}

/// Offers an available update, shows its progress, then a restart.
fn update_item(ui: &mut egui::Ui, p: &Palette, update: &Status) -> Option<Click> {
    match update {
        Status::Available(available) => {
            let mut notes: String = available.notes.chars().take(600).collect();
            if notes.len() < available.notes.len() {
                notes.push('…');
            }
            let item = widgets::nav_item(
                ui,
                p,
                Icon::Download,
                &format!("Update to {}", available.version),
                "",
                true,
            )
            .on_hover_text(if notes.is_empty() {
                "A new version is available".into()
            } else {
                notes
            });
            item.clicked().then_some(Click::Update)
        }
        Status::Downloading { done, total, .. } => {
            let fraction = total
                .filter(|t| *t > 0)
                .map_or(0.0, |t| *done as f32 / t as f32);
            ui.add(
                egui::ProgressBar::new(fraction)
                    .desired_height(4.0)
                    .fill(p.accent),
            );
            ui.label(RichText::new("Updating…").size(12.0).color(p.muted));
            None
        }
        Status::Ready { version, .. } => {
            let item = widgets::nav_item(ui, p, Icon::Restart, "Restart to update", "", true)
                .on_hover_text(format!("Version {version} is installed"));
            item.clicked().then_some(Click::Restart)
        }
        _ => None,
    }
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
