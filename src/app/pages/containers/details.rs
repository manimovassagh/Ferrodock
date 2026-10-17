//! The details pane: header, actions and the Overview / Logs / Stats tabs.

use eframe::egui::{self, RichText};

use super::{ContainersPage, DetailsTab, actions, logs, ports, processes, stats, status};
use crate::app::components::{details, inspect, widgets};
use crate::app::page::{Intent, PageCtx};
use crate::app::pages::{remove_button, remove_task};
use crate::docker::{ContainerInfo, Resource, time_ago};

pub(super) fn show(page: &mut ContainersPage, ui: &mut egui::Ui, ctx: &mut PageCtx) {
    let p = ctx.palette;
    let Some(c) = page.selected(ctx.store) else {
        return;
    };

    if details::header(ui, p, &c.name) {
        page.selected = None;
    }
    ui.horizontal(|ui| {
        status::pill(ui, p, c, ctx.store.pending.get(&c.id).copied());
        ui.label(RichText::new(&c.status).color(p.muted));
    });
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        let busy = ctx.store.pending.contains_key(&c.id);
        if let Some(action) = actions::buttons(ui, c, busy, true) {
            ctx.request(actions::intent(action, c));
        }
        if !busy {
            let blocked = c.is_active().then_some("Stop the container first");
            let task = remove_task(Resource::Containers, &c.id, &c.name);
            remove_button(ui, ctx, "container", task, blocked);
        }
    });
    ui.add_space(12.0);

    widgets::segmented(ui, p, |ui| {
        for (tab, label) in [
            (DetailsTab::Overview, "Overview"),
            (DetailsTab::Logs, "Logs"),
            (DetailsTab::Stats, "Stats"),
            (DetailsTab::Processes, "Top"),
            (DetailsTab::Inspect, "Inspect"),
        ] {
            if widgets::tab(ui, p, label, None, page.tab == tab).clicked() {
                page.tab = tab;
            }
        }
    });
    ui.add_space(10.0);

    let live = ctx.store.live.as_ref();
    match page.tab {
        DetailsTab::Overview => overview(ui, ctx, c),
        DetailsTab::Logs => {
            if let Some(intent) = logs::show(ui, p, live, &mut page.log_view, &c.name) {
                ctx.request(intent);
            }
        }
        DetailsTab::Stats => stats::show(ui, p, live, c.is_running()),
        DetailsTab::Processes => processes::show(ui, ctx, c),
        DetailsTab::Inspect => {
            if inspect::needs_loading(ctx.store, &c.id) {
                ctx.request(Intent::Inspect(Resource::Containers, c.id.clone()));
            }
            egui::ScrollArea::vertical()
                .auto_shrink(false)
                .show(ui, |ui| inspect::show(ui, p, ctx.store, &c.id));
        }
    }
}

/// Overview tab. The image, networks and volumes link to their pages.
fn overview(ui: &mut egui::Ui, ctx: &mut PageCtx, c: &ContainerInfo) {
    let p = ctx.palette;
    let store = ctx.store;
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| {
            details::section(ui, p, "General", |ui| {
                if details::link_field(ui, p, "Image", &[c.image.as_str()]).is_some() {
                    ctx.request(Intent::Open {
                        page: "Images",
                        id: c.image_id.clone(),
                    });
                }
                details::copy_field(ui, p, "ID", c.short_id(), &c.id);
                details::field(ui, p, "Created", &time_ago(c.created));
                details::field(ui, p, "Command", &c.command);
            });
            details::section(ui, p, "Network", |ui| {
                ports::field(ui, p, &c.ports);
                details::field(ui, p, "IP", &widgets::join_or_dash(&c.ip_addresses));
                let networks: Vec<&str> = c.networks.iter().map(String::as_str).collect();
                if let Some(i) = details::link_field(ui, p, "Networks", &networks)
                    && let Some(net) = store.networks.iter().find(|n| n.name == networks[i])
                {
                    ctx.request(Intent::Open {
                        page: "Networks",
                        id: net.id.clone(),
                    });
                }
            });
            details::section(ui, p, "Storage", |ui| {
                if !c.volumes.is_empty() {
                    let volumes: Vec<&str> = c.volumes.iter().map(String::as_str).collect();
                    if let Some(i) = details::link_field(ui, p, "Volumes", &volumes) {
                        ctx.request(Intent::Open {
                            page: "Volumes",
                            id: volumes[i].to_owned(),
                        });
                    }
                }
                details::field(ui, p, "Mounts", &widgets::join_or_dash(&c.mounts));
            });
        });
}
