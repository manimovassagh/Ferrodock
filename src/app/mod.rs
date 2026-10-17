//! The IronStack window: app state, message handling and overall layout.
//! Each area of the window lives in its own submodule.

mod containers;
mod details;
mod sidebar;
mod widgets;

use std::collections::HashSet;
use std::time::{Duration, Instant};

use eframe::egui::{self, Frame, Margin};

use crate::docker::{Action, ContainerInfo};
use crate::theme;
use crate::worker::{Request, Response, Worker};

const REFRESH_INTERVAL: Duration = Duration::from_secs(3);
const TOAST_DURATION: Duration = Duration::from_secs(4);

enum Connection {
    Connecting,
    Connected { version: String },
    Disconnected { reason: String },
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum StateFilter {
    All,
    Running,
    Stopped,
}

impl StateFilter {
    fn matches(self, c: &ContainerInfo) -> bool {
        let active = c.is_running() || c.is_paused();
        match self {
            StateFilter::All => true,
            StateFilter::Running => active,
            StateFilter::Stopped => !active,
        }
    }
}

struct Toast {
    text: String,
    is_error: bool,
    shown_at: Instant,
}

pub struct IronStackApp {
    worker: Worker,
    connection: Connection,
    containers: Vec<ContainerInfo>,
    selected: Option<String>,
    /// Container IDs with an action in flight.
    busy: HashSet<String>,
    search: String,
    state_filter: StateFilter,
    last_refresh: Instant,
    toast: Option<Toast>,
}

impl IronStackApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::install(&cc.egui_ctx);
        let ctx = cc.egui_ctx.clone();
        let worker = Worker::spawn(move || ctx.request_repaint());
        worker.send(Request::Refresh);
        Self {
            worker,
            connection: Connection::Connecting,
            containers: Vec::new(),
            selected: None,
            busy: HashSet::new(),
            search: String::new(),
            state_filter: StateFilter::All,
            last_refresh: Instant::now(),
            toast: None,
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

    fn show_toast(&mut self, text: String, is_error: bool) {
        self.toast = Some(Toast {
            text,
            is_error,
            shown_at: Instant::now(),
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
                    self.show_toast(format!("{past} {name}"), false);
                }
                Response::ActionFailed { id, message } => {
                    self.busy.remove(&id);
                    self.show_toast(message, true);
                }
            }
        }
    }

    fn running_count(&self) -> usize {
        self.containers
            .iter()
            .filter(|c| StateFilter::Running.matches(c))
            .count()
    }

    /// Containers that pass the search box and the state tab.
    fn visible_containers(&self) -> Vec<ContainerInfo> {
        let needle = self.search.trim().to_lowercase();
        self.containers
            .iter()
            .filter(|c| self.state_filter.matches(c))
            .filter(|c| {
                needle.is_empty()
                    || c.name.to_lowercase().contains(&needle)
                    || c.image.to_lowercase().contains(&needle)
                    || c.id.starts_with(&needle)
            })
            .cloned()
            .collect()
    }

    fn selected_container(&self) -> Option<ContainerInfo> {
        let id = self.selected.as_ref()?;
        self.containers.iter().find(|c| &c.id == id).cloned()
    }

    /// Bottom-right notification for action results. Successes fade out on
    /// their own; errors stay until dismissed.
    fn toast(&mut self, ui: &mut egui::Ui) {
        let Some(toast) = &self.toast else { return };
        if !toast.is_error {
            let age = toast.shown_at.elapsed();
            if age >= TOAST_DURATION {
                self.toast = None;
                return;
            }
            ui.ctx().request_repaint_after(TOAST_DURATION - age);
        }

        let p = theme::palette(ui);
        let color = if toast.is_error { p.danger } else { p.success };
        let mut dismissed = false;
        egui::Area::new(egui::Id::unique("ironstack-toast"))
            .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-18.0, -18.0))
            .show(ui.ctx(), |ui| {
                Frame::new()
                    .fill(p.raised)
                    .stroke(egui::Stroke::new(1.0, p.border))
                    .corner_radius(egui::CornerRadius::same(8))
                    .inner_margin(Margin::symmetric(14, 10))
                    .show(ui, |ui| {
                        ui.set_max_width(420.0);
                        ui.horizontal(|ui| {
                            widgets::dot(ui, color);
                            ui.label(egui::RichText::new(&toast.text).color(p.text));
                            if widgets::icon_button(ui, "✕", "Dismiss").clicked() {
                                dismissed = true;
                            }
                        });
                    });
            });
        if dismissed {
            self.toast = None;
        }
    }
}

impl eframe::App for IronStackApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.handle_responses();

        if ui.input(|i| i.key_pressed(egui::Key::F5)) {
            self.refresh();
        }
        if self.last_refresh.elapsed() >= REFRESH_INTERVAL {
            self.refresh();
        }
        ui.ctx().request_repaint_after(REFRESH_INTERVAL);

        let p = theme::palette(ui);
        egui::Panel::left("sidebar")
            .resizable(false)
            .exact_size(210.0)
            .frame(Frame::new().fill(p.sidebar).inner_margin(Margin::same(14)))
            .show(ui, |ui| self.sidebar(ui));

        if let Some(container) = self.selected_container() {
            egui::Panel::right("details")
                .default_size(340.0)
                .min_size(280.0)
                .frame(Frame::new().fill(p.sidebar).inner_margin(Margin::same(18)))
                .show(ui, |ui| self.details(ui, &container));
        }

        egui::CentralPanel::default_margins()
            .frame(
                Frame::new()
                    .fill(p.bg)
                    .inner_margin(Margin::symmetric(22, 18)),
            )
            .show(ui, |ui| self.containers_view(ui));

        self.toast(ui);
    }
}
