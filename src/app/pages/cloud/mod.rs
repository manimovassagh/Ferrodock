//! Cloud emulator page: a local stand-in for AWS, run only when asked.
//! Stopped, the page explains it and starts it; running, it links to the
//! emulator's web console and shows what it has and what it answered.
//!
//! - `launch.rs`: the page while it's stopped, with the Start button.
//! - `status.rs`: the card on top while it runs: console, stop, empty.
//! - `overview.rs`: its services, and how to point tools at it.
//! - `connect.rs`: the settings to copy into each tool.
//! - `services.rs`: the services the emulator has, and their operations.
//! - `requests.rs`: the log of requests it answered, and one in detail.
//! - `pretty.rs`: bodies laid out for reading.

mod connect;
mod launch;
mod overview;
mod pretty;
mod requests;
mod services;
mod status;

use std::time::{Duration, Instant};

use eframe::egui;

use crate::app::components::icons::Icon;
use crate::app::components::widgets;
use crate::app::page::{Command, Intent, Page, PageCtx, Tag};
use crate::app::store::Store;
use crate::app::store::cloud::emulator;
use crate::docker::Resource;
use crate::docker::cloud::{CloudLabels, CloudOp, endpoint};

/// How long to wait before asking an emulator that didn't answer again.
const ASK_AGAIN: Duration = Duration::from_secs(3);

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum Tab {
    #[default]
    Overview,
    Requests,
}

#[derive(Default)]
pub struct CloudPage {
    tab: Tab,
    log: requests::RequestLog,
    overview: overview::Overview,
    /// When the emulator was last asked what it has.
    asked: Option<Instant>,
}

/// The emulator's container, if it's running.
fn running(store: &Store) -> Option<CloudLabels> {
    emulator(&store.containers)
        .filter(|(container, _)| container.is_running())
        .map(|(_, labels)| labels)
}

impl Page for CloudPage {
    fn title(&self) -> &'static str {
        "Cloud emulator"
    }

    fn icon(&self) -> Icon {
        Icon::Cloud
    }

    /// Green while the emulator runs: there's no room for a badge too.
    fn tag(&self, store: &Store) -> Option<Tag> {
        Some(Tag {
            text: "AWS",
            on: running(store).is_some(),
        })
    }

    fn resource(&self) -> Resource {
        Resource::Containers
    }

    fn badge(&self, _store: &Store) -> String {
        String::new()
    }

    fn badge_hint(&self, store: &Store) -> Option<String> {
        running(store)
            .map(|labels| format!("The AWS emulator is running at {}", endpoint(labels.port)))
    }

    fn summary(&self, store: &Store) -> String {
        match running(store) {
            Some(labels) => format!("AWS emulator · running at {}", endpoint(labels.port)),
            None => "Run AWS on your machine, only when you want it".into(),
        }
    }

    fn search_hint(&self) -> &'static str {
        "Filter services and requests"
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
            None => {
                self.asked = None;
                launch::show(ui, ctx);
            }
        }
    }

    fn has_selection(&self, store: &Store) -> bool {
        self.tab == Tab::Requests
            && running(store).is_some()
            && self
                .log
                .selected
                .is_some_and(|id| store.cloud.requests.iter().any(|r| r.id == id))
    }

    fn show_details(&mut self, ui: &mut egui::Ui, ctx: &mut PageCtx) {
        if let Some(labels) = running(ctx.store) {
            self.log.show_details(ui, ctx, labels.port);
        }
    }

    fn clear_selection(&mut self) {
        self.log.selected = None;
    }

    /// Opens the request log with `requests`.
    fn select(&mut self, id: &str) {
        if id == "requests" {
            self.tab = Tab::Requests;
        }
    }

    fn commands(&self, store: &Store) -> Vec<Command> {
        let (label, op) = match running(store) {
            Some(_) => ("Stop the AWS emulator", CloudOp::Stop),
            None => ("Start the AWS emulator", CloudOp::Start(store.cloud.spec)),
        };
        vec![Command::intent(label, "Cloud", Intent::Cloud(op))]
    }
}

impl CloudPage {
    fn running_view(&mut self, ui: &mut egui::Ui, ctx: &mut PageCtx, labels: CloudLabels) {
        let p = ctx.palette;
        self.ask(ui, ctx, labels.port);
        status::show(ui, ctx, labels);
        ui.add_space(10.0);
        widgets::segmented(ui, p, |ui| {
            if widgets::tab(ui, p, "Overview", None, self.tab == Tab::Overview).clicked() {
                self.tab = Tab::Overview;
            }
            let seen = Some(ctx.store.cloud.requests.len()).filter(|n| *n > 0);
            if widgets::tab(ui, p, "Requests", seen, self.tab == Tab::Requests).clicked() {
                self.tab = Tab::Requests;
            }
        });
        ui.add_space(10.0);
        match self.tab {
            Tab::Overview => self.overview.show(ui, ctx, labels),
            Tab::Requests => self.log.show(ui, ctx, labels.port),
        }
    }

    /// Asks what the emulator has once it's up, and again if it didn't
    /// answer.
    fn ask(&mut self, ui: &egui::Ui, ctx: &mut PageCtx, port: u16) {
        let cloud = &ctx.store.cloud;
        let ask = match &cloud.info {
            None => self.asked.is_none(),
            Some(Err(_)) => self.asked.is_none_or(|at| at.elapsed() >= ASK_AGAIN),
            Some(Ok(_)) => false,
        };
        if ask && !cloud.reading && cloud.working.is_none() {
            self.asked = Some(Instant::now());
            ctx.request(Intent::Cloud(CloudOp::Info(port)));
        }
        if matches!(ctx.store.cloud.info, Some(Err(_))) {
            ui.ctx().request_repaint_after(ASK_AGAIN);
        }
    }
}
