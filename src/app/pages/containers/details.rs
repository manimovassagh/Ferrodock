//! The details pane: header, actions and the Overview / Logs / Stats tabs.

use eframe::egui::{self, RichText};

use super::{ContainersPage, DetailsTab, actions, logs, ports, processes, stats, status};
use crate::app::components::{details, inspect, widgets};
use crate::app::page::{Intent, PageCtx};
use crate::app::pages::run_dialog::{self, RunForm};
use crate::app::pages::{remove_button, remove_task};
use crate::docker::{Change, ContainerInfo, Resource, Restart, time_ago};

pub(super) fn show(page: &mut ContainersPage, ui: &mut egui::Ui, ctx: &mut PageCtx) {
    let p = ctx.palette;
    let Some(c) = page.selected(ctx.store) else {
        return;
    };
    dialogs(page, ui, ctx, c);
    // Duplicate and the restart policy need the inspect output.
    if inspect::needs_loading(ctx.store, &c.id) {
        ctx.request(Intent::Inspect(Resource::Containers, c.id.clone()));
    }
    let inspection = ctx
        .store
        .inspection
        .as_ref()
        .filter(|_| ctx.store.inspecting.as_deref() == Some(c.id.as_str()))
        .and_then(|r| r.as_ref().ok());

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
    ui.horizontal(|ui| {
        if ui.small_button("Rename…").clicked() {
            page.rename = Some(c.name.clone());
        }
        let duplicate = ui
            .add_enabled(
                inspection.is_some(),
                egui::Button::new("Duplicate…").small(),
            )
            .on_hover_text("Run another container set up like this one")
            .on_disabled_hover_text("Loading its settings…");
        if duplicate.clicked()
            && let Some(i) = inspection
        {
            page.run_form = Some(RunForm::duplicate(c, i));
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
        DetailsTab::Overview => overview(ui, ctx, c, inspection.map(|i| i.restart_policy.clone())),
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

/// The Rename and Duplicate dialogs, while open.
fn dialogs(page: &mut ContainersPage, ui: &mut egui::Ui, ctx: &mut PageCtx, c: &ContainerInfo) {
    if let Some(form) = &mut page.run_form {
        match run_dialog::show(ui, ctx.palette, form, ctx.store) {
            Some(run_dialog::Outcome::Run(spec)) => {
                ctx.request(Intent::RunImage(spec));
                page.run_form = None;
            }
            Some(run_dialog::Outcome::Cancel) => page.run_form = None,
            None => {}
        }
    }
    if let Some(name) = &mut page.rename {
        match rename_dialog(ui, ctx.palette, &c.name, name) {
            Some(true) => {
                ctx.request(Intent::Change(Change::Rename {
                    id: c.id.clone(),
                    from: c.name.clone(),
                    to: name.trim().to_owned(),
                }));
                page.rename = None;
            }
            Some(false) => page.rename = None,
            None => {}
        }
    }
}

/// Asks for a new name. Some(true) to rename, Some(false) to cancel.
fn rename_dialog(
    ui: &mut egui::Ui,
    p: &crate::theme::Palette,
    old: &str,
    name: &mut String,
) -> Option<bool> {
    let mut choice = None;
    let valid = {
        let n = name.trim();
        !n.is_empty()
            && n != old
            && n.chars()
                .all(|c| c.is_ascii_alphanumeric() || "_.-".contains(c))
    };
    let modal = egui::Modal::new(egui::Id::unique("ferrodock-rename"))
        .frame(
            egui::Frame::new()
                .fill(p.sidebar)
                .stroke(egui::Stroke::new(1.0, p.border))
                .corner_radius(egui::CornerRadius::same(12))
                .inner_margin(egui::Margin::same(20)),
        )
        .show(ui.ctx(), |ui| {
            ui.set_width(360.0);
            ui.label(
                RichText::new(format!("Rename {old}"))
                    .font(egui::FontId::new(18.0, crate::theme::semibold()))
                    .color(p.text),
            );
            ui.add_space(10.0);
            let field = ui.add(egui::TextEdit::singleline(name).desired_width(f32::INFINITY));
            field.request_focus();
            ui.add_space(12.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let rename = ui
                    .add_enabled_ui(valid, |ui| widgets::primary_button(ui, "Rename"))
                    .inner;
                if valid && (rename.clicked() || ui.input(|i| i.key_pressed(egui::Key::Enter))) {
                    choice = Some(true);
                }
                if ui.button("Cancel").clicked() {
                    choice = Some(false);
                }
            });
        });
    if choice.is_none() && modal.should_close() {
        choice = Some(false);
    }
    choice
}

/// Overview tab. The image, networks and volumes link to their pages, and
/// the restart policy can be changed in place once it's known.
fn overview(
    ui: &mut egui::Ui,
    ctx: &mut PageCtx,
    c: &ContainerInfo,
    restart: Option<Option<String>>,
) {
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
                if let Some(current) = restart {
                    let current = Restart::from_name(current.as_deref().unwrap_or("no"));
                    ui.label(RichText::new("Restart").color(p.muted));
                    let mut picked = current;
                    egui::ComboBox::from_id_salt(("restart", &c.id))
                        .selected_text(picked.as_str())
                        .show_ui(ui, |ui| {
                            for policy in Restart::ALL {
                                ui.selectable_value(&mut picked, policy, policy.as_str());
                            }
                        });
                    ui.end_row();
                    if picked != current {
                        ctx.request(Intent::Change(Change::SetRestart {
                            id: c.id.clone(),
                            name: c.name.clone(),
                            policy: picked,
                        }));
                    }
                }
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
