//! Networks page: Docker networks, their addressing and attached containers.

use eframe::egui;

use super::{confirm_removal, remove_button, remove_task, remove_unused_button};
use crate::app::components::{details, table, widgets};
use crate::app::page::{Command, Page, PageCtx, toggle_selection};
use crate::app::store::Store;
use crate::docker::{NetworkInfo, Resource, Task, plural, time_ago};

#[derive(Default)]
pub struct NetworksPage {
    selected: Option<String>,
}

impl NetworksPage {
    fn selected<'a>(&self, store: &'a Store) -> Option<&'a NetworkInfo> {
        let id = self.selected.as_deref()?;
        store.networks.iter().find(|n| n.id == id)
    }
}

impl Page for NetworksPage {
    fn title(&self) -> &'static str {
        "Networks"
    }

    fn resource(&self) -> Resource {
        Resource::Networks
    }

    fn badge(&self, store: &Store) -> String {
        store.networks.len().to_string()
    }

    fn summary(&self, store: &Store) -> String {
        let custom = store.networks.iter().filter(|n| !n.is_builtin()).count();
        format!(
            "{} · {custom} custom",
            plural(store.networks.len(), "network")
        )
    }

    fn search_hint(&self) -> &'static str {
        "Search name, driver or subnet"
    }

    fn toolbar(&mut self, ui: &mut egui::Ui, ctx: &mut PageCtx) {
        remove_unused_button(ui, ctx, "network", unused(ctx.store));
    }

    fn show(&mut self, ui: &mut egui::Ui, ctx: &mut PageCtx) {
        let p = ctx.palette;
        let store = ctx.store;
        let visible: Vec<&NetworkInfo> = store
            .networks
            .iter()
            .filter(|n| ctx.matches_search(&[&n.name, &n.driver, &n.subnets.join(" ")]))
            .collect();
        if visible.is_empty() {
            widgets::empty_state(ui, p, "No networks match.");
            return;
        }

        let columns = vec![
            table::Col::fill("NAME"),
            table::Col::fixed("SUBNET", 150.0),
            table::Col::fixed("CONTAINERS", 100.0),
            table::Col::fixed("CREATED", 130.0),
        ];
        let selected = self.selected.clone();
        let clicked = table::show(
            ui,
            p,
            "networks",
            columns,
            visible.len(),
            |i| selected.as_deref() == Some(visible[i].id.as_str()),
            |i, row| {
                let net = visible[i];
                let attached = store.containers_on_network(&net.name).len();
                row.col(|ui| table::two_line(ui, p, &net.name, &net.driver));
                row.col(|ui| table::text(ui, p, &widgets::join_or_dash(&net.subnets)));
                row.col(|ui| table::text(ui, p, &attached.to_string()));
                row.col(|ui| table::muted(ui, p, &net.created.map(time_ago).unwrap_or_default()));
            },
        );
        if let Some(i) = clicked {
            toggle_selection(&mut self.selected, visible[i].id.clone());
        }
    }

    fn has_selection(&self, store: &Store) -> bool {
        self.selected(store).is_some()
    }

    fn show_details(&mut self, ui: &mut egui::Ui, ctx: &mut PageCtx) {
        let p = ctx.palette;
        let Some(net) = self.selected(ctx.store) else {
            return;
        };
        let attached = ctx.store.containers_on_network(&net.name);

        if details::header(ui, p, &net.name) {
            self.selected = None;
        }
        if net.is_builtin() {
            widgets::pill(ui, "Built-in", p.accent);
        }
        ui.add_space(10.0);
        let blocked = if net.is_builtin() {
            Some("Docker's built-in networks can't be removed")
        } else if !attached.is_empty() {
            Some("Containers are attached to it")
        } else {
            None
        };
        let task = remove_task(Resource::Networks, &net.id, &net.name);
        if !ctx.store.pending.contains(&net.id) {
            remove_button(ui, ctx, "network", task, blocked);
        } else {
            ui.spinner();
        }
        ui.add_space(16.0);

        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                details::section(ui, p, "Overview", |ui| {
                    details::copy_field(ui, p, "ID", net.short_id(), &net.id);
                    details::field(ui, p, "Driver", &net.driver);
                    details::field(ui, p, "Scope", &net.scope);
                    details::field(ui, p, "Internal", if net.internal { "Yes" } else { "No" });
                    details::field(
                        ui,
                        p,
                        "Created",
                        &net.created.map(time_ago).unwrap_or_default(),
                    );
                });
                details::section(ui, p, "Addressing", |ui| {
                    details::field(ui, p, "Subnet", &widgets::join_or_dash(&net.subnets));
                    details::field(ui, p, "Gateway", &widgets::join_or_dash(&net.gateways));
                });
                details::section(ui, p, "Connected", |ui| {
                    details::field(ui, p, "Containers", &widgets::join_or_dash(&attached));
                });
            });
    }

    fn clear_selection(&mut self) {
        self.selected = None;
    }

    fn select(&mut self, id: &str) {
        self.selected = Some(id.to_owned());
    }

    fn commands(&self, store: &Store) -> Vec<Command> {
        let mut list: Vec<Command> = store
            .networks
            .iter()
            .map(|n| Command::select(&n.name, format!("Network · {}", n.driver), &n.id))
            .collect();
        let unused = unused(store);
        if !unused.is_empty() {
            list.push(Command::intent(
                "Remove unused networks",
                plural(unused.len(), "network"),
                confirm_removal("network", unused),
            ));
        }
        list
    }
}

/// Removal tasks for every network no container is on (except Docker's
/// built-in ones).
fn unused(store: &Store) -> Vec<Task> {
    store
        .networks
        .iter()
        .filter(|n| !n.is_builtin() && store.containers_on_network(&n.name).is_empty())
        .map(|n| remove_task(Resource::Networks, &n.id, &n.name))
        .collect()
}
