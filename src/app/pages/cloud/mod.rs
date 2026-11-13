//! Cloud page: the local cloud emulator, run only when asked. While it's
//! up the page shows where it is, what to tell each tool to use it, and
//! what it supports.
//!
//! - `connect.rs`: the settings to copy into each tool.
//! - `services.rs`: the services the emulator has, and their operations.

mod connect;
mod services;

use std::collections::HashSet;
use std::time::{Duration, Instant};

use eframe::egui::{self, RichText};

use crate::app::components::icons::{self, Icon};
use crate::app::components::widgets;
use crate::app::page::{Command, Confirmation, Intent, Page, PageCtx};
use crate::app::store::Store;
use crate::app::store::cloud::emulator;
use crate::docker::Resource;
use crate::docker::cloud::{
    CLOUD_ACCOUNT, CLOUD_REGION, CLOUD_VOLUME, CloudLabels, CloudOp, endpoint,
};
use crate::theme::Palette;

/// How long to wait before asking an emulator that didn't answer again.
const ASK_AGAIN: Duration = Duration::from_secs(3);

#[derive(Default)]
pub struct CloudPage {
    /// The snippet shown in the connect panel.
    snippet: usize,
    /// What was last copied, for its button to say so.
    copied: connect::Copied,
    /// Services unfolded to show their operations, by name.
    unfolded: HashSet<String>,
    /// When the emulator was last asked what it has.
    asked: Option<Instant>,
}

/// The emulator's container, if it's running.
fn running(store: &Store) -> Option<CloudLabels> {
    emulator(&store.containers)
        .filter(|(container, _)| container.is_running())
        .map(|(_, labels)| labels)
}

fn section(ui: &mut egui::Ui, p: &Palette, title: &str) {
    ui.label(
        RichText::new(title)
            .font(egui::FontId::new(11.5, crate::theme::semibold()))
            .color(p.muted),
    );
    ui.add_space(6.0);
}

fn card<R>(ui: &mut egui::Ui, p: &Palette, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::new()
        .fill(p.bg)
        .stroke(egui::Stroke::new(1.0, p.border))
        .corner_radius(egui::CornerRadius::same(10))
        .inner_margin(egui::Margin::symmetric(12, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

/// Asks, then empties the emulator.
fn confirm_reset(port: u16) -> Intent {
    Intent::Confirm(Confirmation {
        title: "Empty the cloud emulator?".into(),
        message: "Everything created in it is deleted: every role, user, policy and whatever \
                  else its services hold. The emulator keeps running. This can't be undone."
            .into(),
        action_label: "Empty".into(),
        intents: vec![Intent::Cloud(CloudOp::Reset(port))],
        ..Default::default()
    })
}

/// Asks, then removes the stopped emulator's container and its data.
fn confirm_remove() -> Intent {
    Intent::Confirm(Confirmation {
        title: "Remove the cloud emulator's data?".into(),
        message: format!(
            "Its container and the volume {CLOUD_VOLUME} are deleted, with everything created \
             in the emulator. It starts empty next time. This can't be undone."
        ),
        action_label: "Remove".into(),
        intents: vec![Intent::Cloud(CloudOp::Remove)],
        ..Default::default()
    })
}

impl Page for CloudPage {
    fn title(&self) -> &'static str {
        "Cloud"
    }

    fn icon(&self) -> Icon {
        Icon::Cloud
    }

    fn resource(&self) -> Resource {
        Resource::Containers
    }

    fn badge(&self, store: &Store) -> String {
        if running(store).is_some() {
            "On".into()
        } else {
            String::new()
        }
    }

    fn badge_hint(&self, store: &Store) -> Option<String> {
        running(store)
            .map(|labels| format!("The cloud emulator is running at {}", endpoint(labels.port)))
    }

    fn summary(&self, store: &Store) -> String {
        match running(store) {
            Some(labels) => format!("AWS emulator · running at {}", endpoint(labels.port)),
            None => "A local stand-in for AWS, only when you want it".into(),
        }
    }

    fn search_hint(&self) -> &'static str {
        "Filter services and operations"
    }

    fn bottom(&self) -> bool {
        true
    }

    fn heading(&self) -> &'static str {
        "CLOUD"
    }

    fn show(&mut self, ui: &mut egui::Ui, ctx: &mut PageCtx) {
        match running(ctx.store) {
            // Stopping shows the running view until it's down.
            Some(labels) => self.running_view(ui, ctx, labels),
            None => self.stopped_view(ui, ctx),
        }
    }

    fn has_selection(&self, _store: &Store) -> bool {
        false
    }

    fn show_details(&mut self, _ui: &mut egui::Ui, _ctx: &mut PageCtx) {}

    fn clear_selection(&mut self) {}

    fn select(&mut self, _id: &str) {}

    fn commands(&self, store: &Store) -> Vec<Command> {
        let (label, op) = match running(store) {
            Some(_) => ("Stop the cloud emulator", CloudOp::Stop),
            None => ("Start the cloud emulator", CloudOp::Start(store.cloud.spec)),
        };
        vec![Command::intent(label, "Cloud", Intent::Cloud(op))]
    }
}

impl CloudPage {
    fn stopped_view(&mut self, ui: &mut egui::Ui, ctx: &mut PageCtx) {
        let p = ctx.palette;
        let cloud = &ctx.store.cloud;
        self.asked = None;
        if let Some(CloudOp::Start(_)) = cloud.working {
            ui.add_space(96.0);
            ui.vertical_centered(|ui| {
                ui.spinner();
                ui.add_space(10.0);
                let step = if cloud.step.is_empty() {
                    "Starting…"
                } else {
                    &cloud.step
                };
                ui.label(RichText::new(step).color(p.muted));
            });
            return;
        }
        let spec = cloud.spec;
        let hint = format!(
            "Speaks the AWS APIs on port {}, so the AWS CLI, the SDKs and Terraform work \
             against it unchanged. Nothing runs until you start it.",
            spec.port
        );
        if widgets::empty_state(
            ui,
            p,
            Icon::Cloud,
            "The cloud emulator isn't running",
            &hint,
            Some("Start"),
        ) {
            ctx.request(Intent::Cloud(CloudOp::Start(spec)));
        }
        ui.vertical_centered(|ui| {
            if let Some(error) = &cloud.error {
                ui.add_space(12.0);
                ui.label(RichText::new(error).color(p.danger));
            }
            // A stopped emulator that kept its data can let go of it.
            let kept = emulator(&ctx.store.containers).is_some_and(|(_, labels)| labels.keep_data);
            if kept {
                ui.add_space(12.0);
                ui.label(
                    RichText::new("What you created is kept, and is there when it starts again.")
                        .size(12.5)
                        .color(p.muted),
                );
                ui.add_space(4.0);
                if cloud.working.is_some() {
                    ui.spinner();
                } else if ui.link("Remove its data…").clicked() {
                    ctx.request(confirm_remove());
                }
            }
        });
    }

    fn running_view(&mut self, ui: &mut egui::Ui, ctx: &mut PageCtx, labels: CloudLabels) {
        let p = ctx.palette;
        let cloud = &ctx.store.cloud;
        let port = labels.port;
        // Ask what it has once it's up, and again if it didn't answer.
        let ask = match &cloud.info {
            None => self.asked.is_none(),
            Some(Err(_)) => self.asked.is_none_or(|at| at.elapsed() >= ASK_AGAIN),
            Some(Ok(_)) => false,
        };
        if ask && !cloud.reading && cloud.working.is_none() {
            self.asked = Some(Instant::now());
            ctx.request(Intent::Cloud(CloudOp::Info(port)));
        }
        if matches!(cloud.info, Some(Err(_))) {
            ui.ctx().request_repaint_after(ASK_AGAIN);
        }

        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                card(ui, p, |ui| {
                    ui.horizontal(|ui| self.status_row(ui, ctx, labels));
                });
                ui.add_space(4.0);
                let version = match &cloud.info {
                    Some(Ok(info)) => format!("ferrocloud {} · ", info.version),
                    _ => String::new(),
                };
                let data = if labels.keep_data {
                    format!("data is kept between starts, in the volume {CLOUD_VOLUME}")
                } else {
                    "data isn't kept: it starts empty each time".to_owned()
                };
                ui.label(
                    RichText::new(format!("{version}{data}"))
                        .size(12.0)
                        .color(p.muted),
                );

                ui.add_space(18.0);
                section(ui, p, "CONNECT");
                connect::show(ui, p, port, &mut self.snippet, &mut self.copied);

                ui.add_space(18.0);
                section(ui, p, "SERVICES");
                services::show(ui, ctx, &mut self.unfolded);
                ui.add_space(8.0);
            });
    }

    fn status_row(&mut self, ui: &mut egui::Ui, ctx: &mut PageCtx, labels: CloudLabels) {
        let p = ctx.palette;
        let (rect, _) = ui.allocate_exact_size(egui::vec2(30.0, 30.0), egui::Sense::hover());
        ui.painter().rect_filled(
            rect,
            egui::CornerRadius::same(8),
            p.accent.gamma_multiply(0.12),
        );
        icons::paint(ui.painter(), rect.shrink(6.0), Icon::Cloud, p.accent);
        let url = endpoint(labels.port);
        ui.vertical(|ui| {
            ui.set_max_width((ui.available_width() - 280.0).max(160.0));
            ui.label(RichText::new("AWS emulator").strong().color(p.text));
            let about = format!("{url} · account {CLOUD_ACCOUNT} · {CLOUD_REGION}");
            ui.add(egui::Label::new(RichText::new(about).size(12.0).color(p.muted)).truncate());
        });
        ui.with_layout(
            egui::Layout::right_to_left(egui::Align::Center),
            |ui| match ctx.store.cloud.working {
                Some(op) => {
                    let doing = match op {
                        CloudOp::Stop => "Stopping…",
                        CloudOp::Reset(_) => "Emptying…",
                        _ => "Working…",
                    };
                    ui.label(RichText::new(doing).size(12.5).color(p.muted));
                    ui.spinner();
                }
                None => {
                    if widgets::icon_button(ui, Icon::Stop, "Stop the emulator").clicked() {
                        ctx.request(Intent::Cloud(CloudOp::Stop));
                    }
                    if ui
                        .button("Empty…")
                        .on_hover_text("Delete everything created in the emulator")
                        .clicked()
                    {
                        ctx.request(confirm_reset(labels.port));
                    }
                    if ui.button("Copy endpoint").clicked() {
                        ui.ctx().copy_text(url);
                        ctx.request(Intent::Notify {
                            text: "Copied the endpoint".into(),
                            is_error: false,
                        });
                    }
                    widgets::pill(ui, "Running", p.success);
                }
            },
        );
    }
}
