//! The IronStack window: container list, details pane and status bar.

use std::collections::HashSet;
use std::time::{Duration, Instant};

use eframe::egui::{self, Color32, RichText};

use crate::docker::{Action, ContainerInfo, time_ago};
use crate::worker::{Request, Response, Worker};

const AUTO_REFRESH_INTERVAL: Duration = Duration::from_secs(3);

enum Connection {
    Connecting,
    Connected { version: String },
    Disconnected { reason: String },
}

struct Notice {
    text: String,
    is_error: bool,
}

pub struct IronStackApp {
    worker: Worker,
    connection: Connection,
    containers: Vec<ContainerInfo>,
    selected: Option<String>,
    /// Container IDs with an action in flight.
    busy: HashSet<String>,
    filter: String,
    auto_refresh: bool,
    last_refresh: Instant,
    notice: Option<Notice>,
}

impl IronStackApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let ctx = cc.egui_ctx.clone();
        let worker = Worker::spawn(move || ctx.request_repaint());
        worker.send(Request::Refresh);
        Self {
            worker,
            connection: Connection::Connecting,
            containers: Vec::new(),
            selected: None,
            busy: HashSet::new(),
            filter: String::new(),
            auto_refresh: true,
            last_refresh: Instant::now(),
            notice: None,
        }
    }

    fn refresh(&mut self) {
        self.last_refresh = Instant::now();
        self.worker.send(Request::Refresh);
    }

    fn run_action(&mut self, action: Action, container: &ContainerInfo) {
        self.busy.insert(container.id.clone());
        self.worker.send(Request::Container {
            action,
            id: container.id.clone(),
            name: container.name.clone(),
        });
    }

    fn handle_responses(&mut self) {
        while let Some(response) = self.worker.try_recv() {
            match response {
                Response::Connected { version } => {
                    self.connection = Connection::Connected { version };
                }
                Response::Containers(list) => {
                    if let Some(id) = &self.selected
                        && !list.iter().any(|c| &c.id == id)
                    {
                        self.selected = None;
                    }
                    self.containers = list;
                }
                Response::Disconnected(reason) => {
                    self.connection = Connection::Disconnected { reason };
                    self.containers.clear();
                    self.busy.clear();
                }
                Response::ActionDone { id, action, name } => {
                    self.busy.remove(&id);
                    let past = match action {
                        Action::Start => "Started",
                        Action::Stop => "Stopped",
                        Action::Restart => "Restarted",
                    };
                    self.notice = Some(Notice {
                        text: format!("{past} {name}"),
                        is_error: false,
                    });
                }
                Response::ActionFailed { id, message } => {
                    self.busy.remove(&id);
                    self.notice = Some(Notice {
                        text: message,
                        is_error: true,
                    });
                }
            }
        }
    }

    fn visible_containers(&self) -> Vec<ContainerInfo> {
        let needle = self.filter.trim().to_lowercase();
        self.containers
            .iter()
            .filter(|c| {
                needle.is_empty()
                    || c.name.to_lowercase().contains(&needle)
                    || c.image.to_lowercase().contains(&needle)
                    || c.id.starts_with(&needle)
            })
            .cloned()
            .collect()
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading(RichText::new("IronStack").strong());
            ui.label(RichText::new("Containers").weak());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .button("⟳ Refresh")
                    .on_hover_text("Reload the container list (F5)")
                    .clicked()
                {
                    self.refresh();
                }
                ui.checkbox(&mut self.auto_refresh, "Auto-refresh");
                ui.add(
                    egui::TextEdit::singleline(&mut self.filter)
                        .hint_text("Filter by name, image or ID")
                        .desired_width(220.0),
                );
            });
        });
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            match &self.connection {
                Connection::Connecting => {
                    ui.spinner();
                    ui.label("Connecting to Docker…");
                }
                Connection::Connected { version } => {
                    ui.colored_label(Color32::from_rgb(46, 160, 67), "●");
                    ui.label(format!("Docker Engine {version}"));
                }
                Connection::Disconnected { .. } => {
                    ui.colored_label(Color32::from_rgb(218, 54, 51), "●");
                    ui.label("Docker Engine not reachable");
                }
            }
            ui.separator();
            let running = self.containers.iter().filter(|c| c.is_running()).count();
            ui.label(format!(
                "{running} running / {} total",
                self.containers.len()
            ));

            if let Some(notice) = &self.notice {
                ui.separator();
                let color = if notice.is_error {
                    Color32::from_rgb(218, 54, 51)
                } else {
                    ui.visuals().text_color()
                };
                ui.colored_label(color, &notice.text);
                if ui.small_button("✕").on_hover_text("Dismiss").clicked() {
                    self.notice = None;
                }
            }
        });
    }

    fn container_list(&mut self, ui: &mut egui::Ui) {
        if let Connection::Disconnected { reason } = &self.connection {
            let reason = reason.clone();
            ui.add_space(40.0);
            ui.vertical_centered(|ui| {
                ui.heading("Can't connect to Docker");
                ui.add_space(8.0);
                ui.label(&reason);
                ui.add_space(8.0);
                ui.label("Start Docker Desktop (or another Docker Engine), then retry.");
                ui.add_space(12.0);
                if ui.button("Retry").clicked() {
                    self.connection = Connection::Connecting;
                    self.refresh();
                }
            });
            return;
        }

        let visible = self.visible_containers();
        if visible.is_empty() {
            ui.add_space(40.0);
            ui.vertical_centered(|ui| {
                if self.containers.is_empty() {
                    ui.label(
                        "No containers yet. Create one with `docker run` and it will show up here.",
                    );
                } else {
                    ui.label("No containers match the filter.");
                }
            });
            return;
        }

        let mut pending: Option<(Action, ContainerInfo)> = None;
        egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| {
            egui::Grid::new("containers")
                .striped(true)
                .num_columns(6)
                .spacing([16.0, 8.0])
                .show(ui, |ui| {
                    for title in ["", "Name", "ID", "Image", "Status", "Actions"] {
                        ui.label(RichText::new(title).strong());
                    }
                    ui.end_row();

                    for c in &visible {
                        ui.colored_label(state_color(&c.state), "●")
                            .on_hover_text(&c.state);
                        let selected = self.selected.as_deref() == Some(c.id.as_str());
                        let name =
                            egui::Button::selectable(selected, RichText::new(&c.name).strong())
                                .truncate();
                        let name_clicked = ui
                            .allocate_ui(egui::vec2(220.0, ui.spacing().interact_size.y), |ui| {
                                ui.add(name).on_hover_text(&c.name).clicked()
                            })
                            .inner;
                        if name_clicked {
                            self.selected = if selected { None } else { Some(c.id.clone()) };
                        }
                        ui.label(RichText::new(c.short_id()).monospace());
                        truncated_label(ui, &c.image, 180.0);
                        truncated_label(ui, &c.status, 170.0);
                        ui.horizontal(|ui| {
                            if let Some(action) = action_buttons(ui, c, self.busy.contains(&c.id)) {
                                pending = Some((action, c.clone()));
                            }
                        });
                        ui.end_row();
                    }
                });
        });

        if let Some((action, container)) = pending {
            self.run_action(action, &container);
        }
    }

    fn details(&mut self, ui: &mut egui::Ui) {
        let Some(c) = self
            .selected
            .as_ref()
            .and_then(|id| self.containers.iter().find(|c| &c.id == id))
            .cloned()
        else {
            return;
        };

        ui.horizontal(|ui| {
            ui.colored_label(state_color(&c.state), "●");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("✕").on_hover_text("Close").clicked() {
                    self.selected = None;
                }
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.add(egui::Label::new(RichText::new(&c.name).heading()).truncate());
                });
            });
        });
        ui.horizontal(|ui| {
            if let Some(action) = action_buttons(ui, &c, self.busy.contains(&c.id)) {
                self.run_action(action, &c);
            }
        });
        ui.separator();

        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                egui::Grid::new("details")
                    .num_columns(2)
                    .spacing([12.0, 6.0])
                    .show(ui, |ui| {
                        detail_row(ui, "State", &c.state);
                        detail_row(ui, "Status", &c.status);
                        detail_row(ui, "Image", &c.image);
                        ui.label(RichText::new("ID").weak());
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(c.short_id()).monospace())
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
                        detail_row(ui, "Created", &time_ago(c.created));
                        detail_row(ui, "Command", &c.command);
                        detail_list(ui, "Ports", &c.ports);
                        detail_list(ui, "IP", &c.ip_addresses);
                        detail_list(ui, "Mounts", &c.mounts);
                    });
            });
    }
}

impl eframe::App for IronStackApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.handle_responses();

        if ui.input(|i| i.key_pressed(egui::Key::F5)) {
            self.refresh();
        }
        if self.auto_refresh {
            if self.last_refresh.elapsed() >= AUTO_REFRESH_INTERVAL {
                self.refresh();
            }
            ui.ctx().request_repaint_after(AUTO_REFRESH_INTERVAL);
        }

        egui::Panel::top("toolbar").show(ui, |ui| {
            ui.add_space(6.0);
            self.toolbar(ui);
            ui.add_space(6.0);
        });
        egui::Panel::bottom("status").show(ui, |ui| {
            ui.add_space(2.0);
            self.status_bar(ui);
            ui.add_space(2.0);
        });
        if self.selected.is_some() {
            egui::Panel::right("details")
                .default_size(320.0)
                .min_size(240.0)
                .show(ui, |ui| {
                    ui.add_space(6.0);
                    self.details(ui);
                });
        }
        egui::CentralPanel::default_margins().show(ui, |ui| self.container_list(ui));
    }
}

/// Draws the Start/Stop + Restart buttons and returns the clicked action.
fn action_buttons(ui: &mut egui::Ui, c: &ContainerInfo, busy: bool) -> Option<Action> {
    if busy {
        ui.spinner();
        return None;
    }
    let mut clicked = None;
    if c.is_running() || c.is_paused() {
        if ui.button("■ Stop").clicked() {
            clicked = Some(Action::Stop);
        }
    } else if ui.button("▶ Start").clicked() {
        clicked = Some(Action::Start);
    }
    if ui.button("⟳ Restart").clicked() {
        clicked = Some(Action::Restart);
    }
    clicked
}

/// A single-line label capped at `max_width`; the full text shows on hover.
fn truncated_label(ui: &mut egui::Ui, text: &str, max_width: f32) {
    let height = ui.spacing().interact_size.y;
    ui.allocate_ui(egui::vec2(max_width, height), |ui| {
        ui.add(egui::Label::new(text).truncate());
    });
}

fn detail_row(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.label(RichText::new(label).weak());
    ui.add(egui::Label::new(if value.is_empty() { "—" } else { value }).wrap());
    ui.end_row();
}

fn detail_list(ui: &mut egui::Ui, label: &str, values: &[String]) {
    detail_row(ui, label, &values.join("\n"));
}

fn state_color(state: &str) -> Color32 {
    match state {
        "running" => Color32::from_rgb(46, 160, 67),
        "paused" | "restarting" => Color32::from_rgb(210, 153, 34),
        "dead" => Color32::from_rgb(218, 54, 51),
        _ => Color32::GRAY,
    }
}
