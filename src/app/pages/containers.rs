//! Containers page: state tabs, the container table and its details pane.

use eframe::egui::{self, Color32, RichText};

use super::{remove_button, remove_task};
use crate::app::components::{details, table, widgets};
use crate::app::page::{Intent, Page, PageCtx, toggle_selection};
use crate::app::store::Store;
use crate::docker::{Action, ContainerInfo, Resource, Task, time_ago};
use crate::theme::Palette;

#[derive(Clone, Copy, PartialEq, Eq)]
enum StateFilter {
    All,
    Running,
    Stopped,
}

impl StateFilter {
    fn matches(self, c: &ContainerInfo) -> bool {
        match self {
            StateFilter::All => true,
            StateFilter::Running => c.is_active(),
            StateFilter::Stopped => !c.is_active(),
        }
    }
}

pub struct ContainersPage {
    filter: StateFilter,
    selected: Option<String>,
}

impl ContainersPage {
    pub fn new() -> Self {
        Self {
            filter: StateFilter::All,
            selected: None,
        }
    }

    fn state_tabs(&mut self, ui: &mut egui::Ui, p: &Palette, store: &Store) {
        let total = store.containers.len();
        let running = store.running_count();
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            for (filter, label, count) in [
                (StateFilter::All, "All", total),
                (StateFilter::Running, "Running", running),
                (StateFilter::Stopped, "Stopped", total - running),
            ] {
                if widgets::tab(ui, p, label, count, self.filter == filter).clicked() {
                    self.filter = filter;
                }
            }
        });
    }
}

impl Page for ContainersPage {
    fn title(&self) -> &'static str {
        "Containers"
    }

    fn resource(&self) -> Resource {
        Resource::Containers
    }

    fn count(&self, store: &Store) -> usize {
        store.containers.len()
    }

    fn summary(&self, store: &Store) -> String {
        format!(
            "{} running · {} total",
            store.running_count(),
            store.containers.len()
        )
    }

    fn search_hint(&self) -> &'static str {
        "Search name, image or ID"
    }

    fn show(&mut self, ui: &mut egui::Ui, ctx: &mut PageCtx) {
        let p = ctx.palette;
        let store = ctx.store;
        self.state_tabs(ui, p, store);
        ui.add_space(8.0);

        let visible: Vec<&ContainerInfo> = store
            .containers
            .iter()
            .filter(|c| self.filter.matches(c))
            .filter(|c| ctx.matches_search(&[&c.name, &c.image, c.short_id()]))
            .collect();
        if visible.is_empty() {
            let message = if store.containers.is_empty() {
                "No containers yet. Create one with `docker run` and it will show up here."
            } else {
                "No containers match."
            };
            widgets::empty_state(ui, p, message);
            return;
        }

        // Drop the less important columns when space is tight (e.g. with
        // the details pane open); both are still shown in the details pane.
        let width = ui.available_width();
        let show_ports = width >= 700.0;
        let show_id = width >= 560.0;
        let mut columns = vec![table::Col::fill("NAME"), table::Col::fixed("STATUS", 150.0)];
        if show_id {
            columns.push(table::Col::fixed("ID", 110.0));
        }
        if show_ports {
            columns.push(table::Col::fixed("PORTS", 140.0));
        }
        columns.push(table::Col::fixed("", 68.0));

        let selected = self.selected.clone();
        let clicked = table::show(
            ui,
            p,
            "containers",
            columns,
            visible.len(),
            |i| selected.as_deref() == Some(visible[i].id.as_str()),
            |i, row| {
                let c = visible[i];
                row.col(|ui| table::two_line(ui, p, &c.name, &c.image));
                row.col(|ui| status_cell(ui, p, c));
                if show_id {
                    row.col(|ui| table::mono(ui, p, c.short_id()));
                }
                if show_ports {
                    row.col(|ui| ports_cell(ui, p, &c.ports));
                }
                row.col(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    let busy = store.pending.contains(&c.id);
                    if let Some(action) = action_buttons(ui, c, busy, false) {
                        ctx.request(intent(action, c));
                    }
                });
            },
        );
        if let Some(i) = clicked {
            toggle_selection(&mut self.selected, visible[i].id.clone());
        }
    }

    fn has_selection(&self, store: &Store) -> bool {
        self.selected
            .as_deref()
            .and_then(|id| store.container(id))
            .is_some()
    }

    fn show_details(&mut self, ui: &mut egui::Ui, ctx: &mut PageCtx) {
        let p = ctx.palette;
        let Some(c) = self
            .selected
            .as_deref()
            .and_then(|id| ctx.store.container(id))
        else {
            return;
        };

        if details::header(ui, p, &c.name) {
            self.selected = None;
        }
        ui.horizontal(|ui| {
            widgets::pill(ui, &capitalize(&c.state), state_color(p, &c.state));
            ui.label(RichText::new(&c.status).color(p.muted));
        });
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            let busy = ctx.store.pending.contains(&c.id);
            if let Some(action) = action_buttons(ui, c, busy, true) {
                ctx.request(intent(action, c));
            }
            if !busy {
                let blocked = c.is_active().then_some("Stop the container first");
                let task = remove_task(Resource::Containers, &c.id, &c.name);
                remove_button(ui, ctx, "container", task, blocked);
            }
        });
        ui.add_space(16.0);

        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                details::section(ui, p, "Overview", |ui| {
                    details::field(ui, p, "Image", &c.image);
                    details::copy_field(ui, p, "ID", c.short_id(), &c.id);
                    details::field(ui, p, "Created", &time_ago(c.created));
                    details::field(ui, p, "Command", &c.command);
                });
                details::section(ui, p, "Network", |ui| {
                    details::field(ui, p, "Ports", &widgets::join_or_dash(&c.ports));
                    details::field(ui, p, "IP", &widgets::join_or_dash(&c.ip_addresses));
                });
                details::section(ui, p, "Storage", |ui| {
                    details::field(ui, p, "Mounts", &widgets::join_or_dash(&c.mounts));
                });
            });
    }

    fn clear_selection(&mut self) {
        self.selected = None;
    }
}

fn intent(action: Action, c: &ContainerInfo) -> Intent {
    Intent::Run(Task {
        action,
        id: c.id.clone(),
        name: c.name.clone(),
    })
}

/// Start/Stop and Restart controls. Shows a spinner while an action is in
/// flight. Returns the clicked action, if any.
fn action_buttons(
    ui: &mut egui::Ui,
    c: &ContainerInfo,
    busy: bool,
    with_labels: bool,
) -> Option<Action> {
    if busy {
        ui.spinner();
        return None;
    }
    let toggle = if c.is_active() {
        (Action::Stop, "■", "Stop")
    } else {
        (Action::Start, "▶", "Start")
    };
    let mut clicked = None;
    for (action, icon, label) in [toggle, (Action::Restart, "⟳", "Restart")] {
        let response = if with_labels {
            ui.button(format!("{icon}  {label}"))
        } else {
            widgets::icon_button(ui, icon, label)
        };
        if response.clicked() {
            clicked = Some(action);
        }
    }
    clicked
}

fn status_cell(ui: &mut egui::Ui, p: &Palette, c: &ContainerInfo) {
    ui.vertical(|ui| {
        ui.add_space(6.0);
        widgets::pill(ui, &capitalize(&c.state), state_color(p, &c.state));
        // Docker's status is just "Created" for new containers, which the
        // pill already says.
        if !c.status.eq_ignore_ascii_case(&c.state) {
            ui.add(
                egui::Label::new(RichText::new(&c.status).size(12.0).color(p.muted))
                    .truncate()
                    .selectable(false),
            );
        }
    });
}

fn ports_cell(ui: &mut egui::Ui, p: &Palette, ports: &[String]) {
    let Some(first) = ports.first() else {
        table::muted(ui, p, "—");
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

fn state_color(p: &Palette, state: &str) -> Color32 {
    match state {
        "running" => p.success,
        "paused" | "restarting" => p.warning,
        "dead" => p.danger,
        _ => p.muted,
    }
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}
