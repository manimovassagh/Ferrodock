//! The card on top while the emulator runs: where it is, the way into its
//! console, and stop and empty.

use eframe::egui::{self, RichText};

use crate::app::components::icons::{self, Icon};
use crate::app::components::widgets;
use crate::app::page::{Confirmation, Intent, PageCtx};
use crate::docker::cloud::{CLOUD_ACCOUNT, CLOUD_REGION, CloudLabels, CloudOp, endpoint};

pub fn show(ui: &mut egui::Ui, ctx: &mut PageCtx, labels: CloudLabels) {
    let p = ctx.palette;
    egui::Frame::new()
        .fill(p.bg)
        .stroke(egui::Stroke::new(1.0, p.border))
        .corner_radius(egui::CornerRadius::same(10))
        .inner_margin(egui::Margin::symmetric(12, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| row(ui, ctx, labels));
        });
}

fn row(ui: &mut egui::Ui, ctx: &mut PageCtx, labels: CloudLabels) {
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
        ui.set_max_width((ui.available_width() - 460.0).max(120.0));
        ui.horizontal(|ui| {
            ui.label(RichText::new("AWS emulator").strong().color(p.text));
            widgets::pill(ui, "Running", p.success);
        });
        let about = format!("{url} · account {CLOUD_ACCOUNT} · {CLOUD_REGION}");
        ui.add(egui::Label::new(RichText::new(about).size(12.0).color(p.muted)).truncate());
    });
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        if let Some(op) = ctx.store.cloud.working {
            let doing = match op {
                CloudOp::Stop => "Stopping…",
                CloudOp::Reset(_) => "Emptying…",
                _ => "Working…",
            };
            ui.label(RichText::new(doing).size(12.5).color(p.muted));
            ui.spinner();
            return;
        }
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
            ui.ctx().copy_text(url.clone());
            ctx.request(Intent::Notify {
                text: "Copied the endpoint".into(),
                is_error: false,
            });
        }
        if let Some(console) = console(ctx, &url)
            && widgets::primary_button(ui, "Open console")
                .on_hover_text(format!("Opens {console} in your browser"))
                .clicked()
        {
            ui.ctx().open_url(egui::OpenUrl::new_tab(console));
        }
    });
}

/// The web console's address, when this emulator has one.
fn console(ctx: &PageCtx, url: &str) -> Option<String> {
    match &ctx.store.cloud.info {
        Some(Ok(info)) => info.console.as_ref().map(|path| format!("{url}{path}")),
        _ => None,
    }
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
