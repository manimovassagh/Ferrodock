//! Networks page: Docker networks, their addressing and attached containers.

use eframe::egui;

use super::create_dialog::{CreateForm, Kind};
use super::marks::{Item, Marks};
use super::{confirm_removal, inspect_section, remove_button, remove_task, remove_unused_button};
use crate::app::components::icons::Icon;
use crate::app::components::{details, table, widgets};
use crate::app::page::{Command, Intent, Page, PageCtx, toggle_selection};
use crate::app::store::Store;
use crate::docker::{Change, NetworkInfo, Resource, Task, plural, time_ago};

#[derive(Default)]
pub struct NetworksPage {
    selected: Option<String>,
    /// The New network dialog, while it's open.
    create: Option<CreateForm>,
    /// The container picked in "Connect a container".
    connect: String,
    /// Rows ticked for removing together.
    marks: Marks,
    /// Whether Docker's own networks (bridge, host, none) are listed too.
    show_builtin: bool,
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

    fn icon(&self) -> Icon {
        Icon::Network
    }

    fn resource(&self) -> Resource {
        Resource::Networks
    }

    /// Your own networks: Docker's built-in ones are always there.
    fn badge(&self, store: &Store) -> String {
        custom_count(store).to_string()
    }

    fn summary(&self, store: &Store) -> String {
        let builtin = store.networks.len() - custom_count(store);
        format!(
            "{} · {builtin} built into Docker",
            plural(custom_count(store), "network")
        )
    }

    fn search_hint(&self) -> &'static str {
        "Search name, driver or subnet"
    }

    fn toolbar(&mut self, ui: &mut egui::Ui, ctx: &mut PageCtx) {
        remove_unused_button(ui, ctx, "network", unused(ctx.store));
        if widgets::button(ui, Icon::Network, "New network").clicked() {
            self.create = Some(CreateForm::new(Kind::Network));
        }
    }

    fn has_overlay(&self) -> bool {
        self.create.is_some()
    }

    fn show(&mut self, ui: &mut egui::Ui, ctx: &mut PageCtx) {
        super::create_dialog(ui, ctx, &mut self.create);
        let p = ctx.palette;
        let store = ctx.store;
        // Docker's own networks only on request (or when searched for):
        // they're always there and can't be changed much.
        let with_builtin = self.show_builtin || !ctx.search_is_empty();
        let mut visible: Vec<&NetworkInfo> = store
            .networks
            .iter()
            .filter(|n| with_builtin || !n.is_builtin())
            .filter(|n| ctx.matches_search(&[&n.name, &n.driver, &n.subnets.join(" ")]))
            .collect();
        // Yours first, then Docker's.
        visible.sort_by_key(|n| n.is_builtin());
        let builtin = store.networks.len() - custom_count(store);
        if visible.is_empty() && ctx.search_is_empty() && !store.networks.is_empty() {
            if widgets::empty_state(
                ui,
                p,
                Icon::Network,
                "No networks of your own yet",
                "Containers join Docker's built-in bridge network unless you make one. \
                 Make one to let a group of containers reach each other by name.",
                Some("New network"),
            ) {
                self.create = Some(CreateForm::new(Kind::Network));
            }
            ui.vertical_centered(|ui| self.builtin_toggle(ui, builtin));
            return;
        }
        if visible.is_empty() {
            super::nothing_to_show(
                ui,
                ctx,
                !store.networks.is_empty(),
                (
                    Icon::Network,
                    "No networks",
                    "Docker's own networks appear here once it's running.",
                ),
                None,
            );
            return;
        }

        let items: Vec<Item> = visible.iter().map(|n| item(store, n)).collect();
        self.marks
            .bar(ui, ctx, "network", Resource::Networks, &items);
        let marks = &self.marks;
        let mut ticks = Vec::new();
        // With the details pane open, the subnet stays longest; all three
        // are in the details pane too.
        let fit = table::fitting(ui, &[28.0], &[150.0, 100.0, 130.0]);
        let (show_subnet, show_attached, show_created) = (fit[0], fit[1], fit[2]);
        let mut columns = vec![marks.column(&items), table::Col::fill("NAME")];
        if show_subnet {
            columns.push(table::Col::fixed("SUBNET", 150.0));
        }
        if show_attached {
            columns.push(table::Col::fixed("CONTAINERS", 100.0));
        }
        if show_created {
            columns.push(table::Col::fixed("CREATED", 130.0));
        }
        let selected = self.selected.clone();
        let clicked = table::show(
            ui,
            p,
            "networks",
            columns,
            visible.len(),
            |i| selected.as_deref() == Some(visible[i].id.as_str()) || marks.contains(items[i].id),
            |i| format!("Network {}", visible[i].name),
            |i, row| {
                let net = visible[i];
                let attached = store.containers_on_network(&net.name).len();
                // Docker's own can't be removed, so there's nothing to tick.
                if net.is_builtin() {
                    row.col(|_| {});
                } else {
                    row.col(|ui| ticks.extend(marks.cell(ui, p, &items[i])));
                }
                let line = builtin_role(&net.name).unwrap_or(&net.driver);
                row.col(|ui| table::with_icon(ui, p, Icon::Network, p.muted, &net.name, line));
                if show_subnet {
                    row.col(|ui| table::text(ui, p, &widgets::join_or_dash(&net.subnets)));
                }
                if show_attached {
                    row.col(|ui| table::text(ui, p, &attached.to_string()));
                }
                if show_created {
                    row.col(|ui| {
                        table::muted(ui, p, &net.created.map(time_ago).unwrap_or_default())
                    });
                }
            },
        );
        self.marks.apply(ui, "networks", &items, ticks);
        if let Some(i) = clicked {
            toggle_selection(&mut self.selected, visible[i].id.clone());
        }
        if ctx.search_is_empty() {
            ui.add_space(8.0);
            self.builtin_toggle(ui, builtin);
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
        if let Some(role) = builtin_role(&net.name) {
            // Nothing to remove: say what it is instead.
            widgets::pill(ui, "Built into Docker", p.muted);
            ui.add_space(6.0);
            ui.label(egui::RichText::new(role).color(p.text));
            ui.label(
                egui::RichText::new("Every Docker engine has it, and it can't be removed.")
                    .size(12.5)
                    .color(p.muted),
            );
        } else {
            ui.add_space(10.0);
            let blocked = (!attached.is_empty()).then_some("Containers are attached to it");
            let task = remove_task(Resource::Networks, &net.id, &net.name);
            if !ctx.store.pending.contains_key(&net.id) {
                remove_button(ui, ctx, "network", task, blocked);
            } else {
                ui.spinner();
            }
        }
        ui.add_space(16.0);

        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                details::section(ui, p, "Overview", |ui| {
                    details::copy_field(ui, p, "ID", net.short_id(), &net.id);
                    details::field(ui, p, "Driver", driver_label(&net.driver));
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
                connected(ui, ctx, net, &attached, &mut self.connect);
                inspect_section(ui, ctx, Resource::Networks, &net.id);
            });
    }

    fn clear_selection(&mut self) {
        self.selected = None;
        self.marks.clear();
    }

    fn select_all(&mut self, ctx: &PageCtx) {
        let items: Vec<Item> = ctx
            .store
            .networks
            .iter()
            .filter(|n| !n.is_builtin())
            .filter(|n| ctx.matches_search(&[&n.name, &n.driver, &n.subnets.join(" ")]))
            .map(|n| item(ctx.store, n))
            .collect();
        self.marks.select_all(&items);
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

impl NetworksPage {
    /// "Show Docker's built-in networks (3)", or Hide them again.
    fn builtin_toggle(&mut self, ui: &mut egui::Ui, count: usize) {
        if count == 0 {
            return;
        }
        let label = if self.show_builtin {
            "Hide Docker's built-in networks".to_owned()
        } else {
            format!("Show Docker's built-in networks ({count})")
        };
        if ui
            .link(label)
            .on_hover_text("bridge, host and none: every Docker engine has them")
            .clicked()
        {
            self.show_builtin = !self.show_builtin;
        }
    }
}

/// How many networks are your own, not Docker's.
fn custom_count(store: &Store) -> usize {
    store.networks.iter().filter(|n| !n.is_builtin()).count()
}

/// What one of Docker's own networks is for, in plain words.
fn builtin_role(name: &str) -> Option<&'static str> {
    Some(match name {
        "bridge" => "The default network: containers join it unless told otherwise.",
        "host" => "Containers on it share this computer's network directly.",
        "none" => "Containers on it have no network at all.",
        _ => return None,
    })
}

/// Docker calls the `none` network's driver "null".
fn driver_label(driver: &str) -> &str {
    if driver == "null" {
        "none (no networking)"
    } else {
        driver
    }
}

/// A network as a row to tick; built-in ones count as in use, since they
/// can't be removed.
fn item<'a>(store: &Store, n: &'a NetworkInfo) -> Item<'a> {
    Item {
        id: &n.id,
        name: &n.name,
        in_use: n.is_builtin() || !store.containers_on_network(&n.name).is_empty(),
    }
}

fn unused(store: &Store) -> Vec<Task> {
    store
        .networks
        .iter()
        .filter(|n| !n.is_builtin() && store.containers_on_network(&n.name).is_empty())
        .map(|n| remove_task(Resource::Networks, &n.id, &n.name))
        .collect()
}

/// The containers on a network, each with Disconnect, and a picker to
/// connect another. Docker's `host` and `none` networks can't be changed.
fn connected(
    ui: &mut egui::Ui,
    ctx: &mut PageCtx,
    net: &NetworkInfo,
    attached: &[&str],
    pick: &mut String,
) {
    let p = ctx.palette;
    let fixed = matches!(net.name.as_str(), "host" | "none");
    details::section(ui, p, "Connected", |ui| {
        if attached.is_empty() {
            details::field(ui, p, "Containers", "");
        }
        for &name in attached {
            ui.label(egui::RichText::new(name).color(p.text));
            let id = format!("{}+{name}", net.name);
            if ctx.store.pending.contains_key(&id) {
                ui.spinner();
            } else if !fixed && ui.small_button("Disconnect").clicked() {
                ctx.request(Intent::Change(Change::Disconnect {
                    network: net.name.clone(),
                    container: name.to_owned(),
                }));
            } else if fixed {
                ui.label("");
            }
            ui.end_row();
        }
    });
    if fixed {
        return;
    }
    let others: Vec<&str> = ctx
        .store
        .containers
        .iter()
        .map(|c| c.name.as_str())
        .filter(|name| !attached.contains(name))
        .collect();
    if others.is_empty() {
        return;
    }
    if !others.contains(&pick.as_str()) {
        *pick = others[0].to_owned();
    }
    ui.horizontal(|ui| {
        egui::ComboBox::from_id_salt(("connect", &net.id))
            .selected_text(pick.as_str())
            .show_ui(ui, |ui| {
                for name in &others {
                    ui.selectable_value(pick, (*name).to_owned(), *name);
                }
            });
        if ui.button("Connect").clicked() {
            ctx.request(Intent::Change(Change::Connect {
                network: net.name.clone(),
                container: pick.clone(),
            }));
        }
    });
    ui.add_space(14.0);
}
