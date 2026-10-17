//! Right-hand pane with information about the selected container.

use eframe::egui::{self, Align, Layout, RichText};

use super::{IronStackApp, widgets};
use crate::docker::{ContainerInfo, time_ago};
use crate::theme;

impl IronStackApp {
    pub(super) fn details(&mut self, ui: &mut egui::Ui, c: &ContainerInfo) {
        let p = theme::palette(ui);

        ui.horizontal(|ui| {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if widgets::icon_button(ui, "✕", "Close").clicked() {
                    self.selected = None;
                }
                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                    ui.add(
                        egui::Label::new(RichText::new(&c.name).size(18.0).strong().color(p.text))
                            .truncate(),
                    );
                });
            });
        });
        ui.horizontal(|ui| {
            widgets::status_pill(ui, p, &c.state);
            ui.label(RichText::new(&c.status).color(p.muted));
        });

        ui.add_space(10.0);
        ui.horizontal(|ui| {
            if let Some(action) = widgets::container_actions(ui, c, self.busy.contains(&c.id), true)
            {
                self.run_action(action, c);
            }
        });
        ui.add_space(16.0);

        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                widgets::section(ui, p, "Overview", |ui| {
                    egui::Grid::new("overview")
                        .num_columns(2)
                        .spacing([14.0, 8.0])
                        .show(ui, |ui| {
                            widgets::field(ui, p, "Image", &c.image);
                            ui.label(RichText::new("ID").color(p.muted));
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(c.short_id()).monospace().color(p.text))
                                    .on_hover_text(&c.id);
                                if ui
                                    .small_button("Copy")
                                    .on_hover_text("Copy full ID")
                                    .clicked()
                                {
                                    ui.ctx().copy_text(c.id.clone());
                                }
                            });
                            ui.end_row();
                            widgets::field(ui, p, "Created", &time_ago(c.created));
                            widgets::field(ui, p, "Command", &c.command);
                        });
                });
                widgets::section(ui, p, "Network", |ui| {
                    egui::Grid::new("network")
                        .num_columns(2)
                        .spacing([14.0, 8.0])
                        .show(ui, |ui| {
                            widgets::field(ui, p, "Ports", &c.ports.join("\n"));
                            widgets::field(ui, p, "IP", &c.ip_addresses.join("\n"));
                        });
                });
                widgets::section(ui, p, "Storage", |ui| {
                    egui::Grid::new("storage")
                        .num_columns(2)
                        .spacing([14.0, 8.0])
                        .show(ui, |ui| {
                            widgets::field(ui, p, "Mounts", &c.mounts.join("\n"));
                        });
                });
            });
    }
}
