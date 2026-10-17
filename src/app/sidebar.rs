//! Left sidebar: brand, navigation and Docker engine status.

use eframe::egui::{self, Align, Layout, RichText};

use super::{Connection, IronStackApp, widgets};
use crate::theme;

impl IronStackApp {
    pub(super) fn sidebar(&mut self, ui: &mut egui::Ui) {
        let p = theme::palette(ui);

        ui.add_space(2.0);
        ui.horizontal(|ui| {
            widgets::logo(ui, p, 22.0);
            ui.label(RichText::new("IronStack").size(17.0).strong().color(p.text));
        });

        ui.add_space(22.0);
        ui.label(RichText::new("DOCKER").size(11.0).color(p.muted));
        ui.add_space(2.0);
        widgets::nav_item(ui, p, "Containers", Some(self.containers.len()), true);

        ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
            ui.horizontal(|ui| match &self.connection {
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
            });
        });
    }
}
