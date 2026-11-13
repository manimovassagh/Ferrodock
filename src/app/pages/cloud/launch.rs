//! The page while the emulator is stopped: what it is, how it's used, and
//! the button that starts it.

use eframe::egui::{self, RichText};

use crate::app::components::icons::Icon;
use crate::app::components::widgets;
use crate::app::page::{Confirmation, Intent, PageCtx};
use crate::app::store::cloud::emulator;
use crate::docker::cloud::{CLOUD_VOLUME, CloudOp};
use crate::theme::Palette;

/// How it's used, in order, under the Start button.
const STEPS: [&str; 3] = [
    "Start it",
    "Open its console in your browser",
    "Point the AWS CLI or your code at it",
];

pub fn show(ui: &mut egui::Ui, ctx: &mut PageCtx) {
    let p = ctx.palette;
    let cloud = &ctx.store.cloud;
    if let Some(CloudOp::Start(_)) = cloud.working {
        starting(ui, p, &cloud.step);
        return;
    }
    let spec = cloud.spec;
    let hint = format!(
        "A local stand-in for AWS. It speaks the AWS APIs on port {}, so the AWS CLI, the \
         SDKs and Terraform work against it unchanged, and nothing leaves this computer. \
         Nothing runs until you start it.",
        spec.port
    );
    if widgets::empty_state(
        ui,
        p,
        Icon::Cloud,
        "Run AWS on your machine",
        &hint,
        Some("Start the AWS emulator"),
    ) {
        ctx.request(Intent::Cloud(CloudOp::Start(spec)));
    }
    ui.add_space(18.0);
    steps(ui, p);
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

fn starting(ui: &mut egui::Ui, p: &Palette, step: &str) {
    ui.add_space(96.0);
    ui.vertical_centered(|ui| {
        ui.spinner();
        ui.add_space(10.0);
        let step = if step.is_empty() { "Starting…" } else { step };
        ui.label(RichText::new(step).color(p.muted));
    });
}

/// The numbered steps, in one centred row.
fn steps(ui: &mut egui::Ui, p: &Palette) {
    let row = |ui: &mut egui::Ui| {
        for (i, step) in STEPS.iter().enumerate() {
            if i > 0 {
                ui.label(RichText::new("→").color(p.muted));
            }
            ui.label(
                RichText::new(format!("{}", i + 1))
                    .size(11.5)
                    .strong()
                    .color(p.accent),
            );
            ui.label(RichText::new(*step).size(12.5).color(p.muted));
        }
    };
    // Measured once off screen, so the row can be centred.
    let width = ui
        .new_child(egui::UiBuilder::new().invisible())
        .horizontal(row)
        .response
        .rect
        .width();
    ui.horizontal(|ui| {
        ui.add_space(((ui.available_width() - width) / 2.0).max(0.0));
        row(ui);
    });
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
