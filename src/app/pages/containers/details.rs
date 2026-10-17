//! The details pane: header, actions and the Overview / Logs / Stats tabs.

use eframe::egui::{self, RichText};

use super::{ContainersPage, DetailsTab, actions, logs, ports, stats, status};
use crate::app::components::{details, inspect, widgets};
use crate::app::page::{Intent, PageCtx};
use crate::app::pages::{remove_button, remove_task};
use crate::docker::{ContainerInfo, Resource, time_ago};
use crate::theme::Palette;

pub(super) fn show(page: &mut ContainersPage, ui: &mut egui::Ui, ctx: &mut PageCtx) {
    let p = ctx.palette;
    let Some(c) = page.selected(ctx.store) else {
        return;
    };

    if details::header(ui, p, &c.name) {
        page.selected = None;
    }
    ui.horizontal(|ui| {
        status::pill(ui, p, c);
        ui.label(RichText::new(&c.status).color(p.muted));
    });
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        let busy = ctx.store.pending.contains(&c.id);
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

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        for (tab, label) in [
            (DetailsTab::Overview, "Overview"),
            (DetailsTab::Logs, "Logs"),
            (DetailsTab::Stats, "Stats"),
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
        DetailsTab::Overview => overview(ui, p, c),
        DetailsTab::Logs => logs::show(ui, p, live, &mut page.log_filter),
        DetailsTab::Stats => stats::show(ui, p, live, c.is_running()),
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

fn overview(ui: &mut egui::Ui, p: &Palette, c: &ContainerInfo) {
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| {
            details::section(ui, p, "General", |ui| {
                details::field(ui, p, "Image", &c.image);
                details::copy_field(ui, p, "ID", c.short_id(), &c.id);
                details::field(ui, p, "Created", &time_ago(c.created));
                details::field(ui, p, "Command", &c.command);
            });
            details::section(ui, p, "Network", |ui| {
                ports::field(ui, p, &c.ports);
                details::field(ui, p, "IP", &widgets::join_or_dash(&c.ip_addresses));
            });
            details::section(ui, p, "Storage", |ui| {
                details::field(ui, p, "Mounts", &widgets::join_or_dash(&c.mounts));
            });
        });
}
