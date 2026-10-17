//! Main view: header, state tabs and the container table.

use eframe::egui::{self, Align, Layout, RichText, Sense};
use egui_extras::{Column, TableBuilder};

use super::{Connection, IronStackApp, StateFilter, widgets};
use crate::docker::{Action, ContainerInfo};
use crate::theme::{self, Palette};

const ROW_HEIGHT: f32 = 48.0;

impl IronStackApp {
    pub(super) fn containers_view(&mut self, ui: &mut egui::Ui) {
        let p = theme::palette(ui);
        self.header(ui, p);
        ui.add_space(12.0);

        if let Connection::Disconnected { reason } = &self.connection {
            let reason = reason.clone();
            self.disconnected(ui, p, &reason);
            return;
        }

        self.state_tabs(ui, p);
        ui.add_space(8.0);

        let visible = self.visible_containers();
        if visible.is_empty() {
            let message = if self.containers.is_empty() {
                "No containers yet. Create one with `docker run` and it will show up here."
            } else {
                "No containers match."
            };
            empty_state(ui, p, message);
            return;
        }
        self.table(ui, p, &visible);
    }

    fn header(&mut self, ui: &mut egui::Ui, p: &Palette) {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(
                    RichText::new("Containers")
                        .size(22.0)
                        .strong()
                        .color(p.text),
                );
                ui.label(
                    RichText::new(format!(
                        "{} running · {} total",
                        self.running_count(),
                        self.containers.len()
                    ))
                    .color(p.muted),
                );
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if widgets::icon_button(ui, "⟳", "Refresh (F5)").clicked() {
                    self.refresh();
                }
                ui.add(
                    egui::TextEdit::singleline(&mut self.search)
                        .hint_text("Search name, image or ID")
                        .desired_width(240.0)
                        .margin(egui::vec2(8.0, 5.0)),
                );
            });
        });
    }

    fn state_tabs(&mut self, ui: &mut egui::Ui, p: &Palette) {
        let total = self.containers.len();
        let running = self.running_count();
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            for (filter, label, count) in [
                (StateFilter::All, "All", total),
                (StateFilter::Running, "Running", running),
                (StateFilter::Stopped, "Stopped", total - running),
            ] {
                if widgets::tab(ui, p, label, count, self.state_filter == filter).clicked() {
                    self.state_filter = filter;
                }
            }
        });
    }

    fn table(&mut self, ui: &mut egui::Ui, p: &Palette, visible: &[ContainerInfo]) {
        let mut clicked_row: Option<String> = None;
        let mut pending: Option<(Action, ContainerInfo)> = None;

        // Drop the less important columns when space is tight (e.g. with the
        // details pane open); both are still shown in the details pane.
        let width = ui.available_width();
        let show_ports = width >= 700.0;
        let show_id = width >= 560.0;

        let mut table = TableBuilder::new(ui)
            .id_salt("containers")
            .sense(Sense::click())
            .cell_layout(Layout::left_to_right(Align::Center))
            .column(Column::remainder().at_least(160.0).clip(true))
            .column(Column::exact(150.0).clip(true));
        let mut titles = vec!["NAME", "STATUS"];
        if show_id {
            table = table.column(Column::exact(110.0));
            titles.push("ID");
        }
        if show_ports {
            table = table.column(Column::exact(140.0).clip(true));
            titles.push("PORTS");
        }
        titles.push("");

        table
            .column(Column::exact(68.0))
            .header(26.0, |mut header| {
                for title in titles {
                    header.col(|ui| {
                        ui.label(RichText::new(title).size(11.0).color(p.muted));
                    });
                }
            })
            .body(|body| {
                body.rows(ROW_HEIGHT, visible.len(), |mut row| {
                    let c = &visible[row.index()];
                    row.set_selected(self.selected.as_deref() == Some(c.id.as_str()));

                    row.col(|ui| {
                        ui.vertical(|ui| {
                            ui.add_space(6.0);
                            ui.add(
                                egui::Label::new(RichText::new(&c.name).strong().color(p.text))
                                    .truncate()
                                    .selectable(false),
                            );
                            ui.add(
                                egui::Label::new(RichText::new(&c.image).size(12.0).color(p.muted))
                                    .truncate()
                                    .selectable(false),
                            );
                        });
                    });
                    row.col(|ui| {
                        ui.vertical(|ui| {
                            ui.add_space(6.0);
                            widgets::status_pill(ui, p, &c.state);
                            // Docker's status is just "Created" for new containers,
                            // which the pill already says.
                            if !c.status.eq_ignore_ascii_case(&c.state) {
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(&c.status).size(12.0).color(p.muted),
                                    )
                                    .truncate()
                                    .selectable(false),
                                );
                            }
                        });
                    });
                    if show_id {
                        row.col(|ui| {
                            ui.add(
                                egui::Label::new(
                                    RichText::new(c.short_id()).monospace().color(p.muted),
                                )
                                .selectable(false),
                            );
                        });
                    }
                    if show_ports {
                        row.col(|ui| ports_cell(ui, p, &c.ports));
                    }
                    row.col(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        if let Some(action) =
                            widgets::container_actions(ui, c, self.busy.contains(&c.id), false)
                        {
                            pending = Some((action, c.clone()));
                        }
                    });

                    if row.response().clicked() {
                        clicked_row = Some(c.id.clone());
                    }
                });
            });

        if let Some((action, container)) = pending {
            self.run_action(action, &container);
        } else if let Some(id) = clicked_row {
            let already = self.selected.as_ref() == Some(&id);
            self.selected = if already { None } else { Some(id) };
        }
    }

    fn disconnected(&mut self, ui: &mut egui::Ui, p: &Palette, reason: &str) {
        ui.add_space(60.0);
        ui.vertical_centered(|ui| {
            ui.label(
                RichText::new("Docker isn't running")
                    .size(18.0)
                    .strong()
                    .color(p.text),
            );
            ui.add_space(6.0);
            ui.label(
                RichText::new(
                    "Start Docker Desktop (or another Docker Engine). \
                     IronStack connects automatically once it's up.",
                )
                .color(p.muted),
            );
            ui.add_space(4.0);
            ui.label(RichText::new(reason).size(12.0).color(p.muted));
            ui.add_space(14.0);
            if ui.button("Retry now").clicked() {
                self.connection = Connection::Connecting;
                self.refresh();
            }
        });
    }
}

fn ports_cell(ui: &mut egui::Ui, p: &Palette, ports: &[String]) {
    let Some(first) = ports.first() else {
        ui.label(RichText::new("—").color(p.muted));
        return;
    };
    let short = first.replace("0.0.0.0:", "").replace(" -> ", " → ");
    let response = ui.add(egui::Label::new(RichText::new(short).color(p.text)).selectable(false));
    if ports.len() > 1 {
        ui.label(
            RichText::new(format!("+{}", ports.len() - 1))
                .size(12.0)
                .color(p.muted),
        );
    }
    response.on_hover_text(ports.join("\n"));
}

fn empty_state(ui: &mut egui::Ui, p: &Palette, message: &str) {
    ui.add_space(60.0);
    ui.vertical_centered(|ui| {
        ui.label(RichText::new(message).color(p.muted));
    });
}
