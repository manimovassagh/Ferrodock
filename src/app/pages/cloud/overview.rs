//! The Overview tab: what the emulator has, then, folded away until
//! asked for, how to point tools at it.

use std::collections::HashSet;

use eframe::egui::{self, RichText};

use super::{connect, services};
use crate::app::components::icons::{self, Icon};
use crate::app::page::PageCtx;
use crate::docker::cloud::{CLOUD_VOLUME, CloudLabels};
use crate::theme::Palette;

#[derive(Default)]
pub struct Overview {
    /// The snippet shown in the connect panel.
    snippet: usize,
    /// What was last copied, for its button to say so.
    copied: connect::Copied,
    /// Services unfolded to show their operations, by name.
    unfolded: HashSet<String>,
    /// Whether the connect panel is unfolded.
    connect_open: bool,
}

impl Overview {
    pub fn show(&mut self, ui: &mut egui::Ui, ctx: &mut PageCtx, labels: CloudLabels) {
        let p = ctx.palette;
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                ui.label(RichText::new(about(ctx, labels)).size(12.0).color(p.muted));

                ui.add_space(18.0);
                section(ui, p, "SERVICES");
                services::show(ui, ctx, &mut self.unfolded);

                ui.add_space(18.0);
                if fold(
                    ui,
                    p,
                    "Use it from the AWS CLI or your code",
                    self.connect_open,
                ) {
                    self.connect_open = !self.connect_open;
                }
                if self.connect_open {
                    ui.add_space(8.0);
                    connect::show(ui, p, labels.port, &mut self.snippet, &mut self.copied);
                }
                ui.add_space(8.0);
            });
    }
}

/// Its version, and whether what's made in it is kept.
fn about(ctx: &PageCtx, labels: CloudLabels) -> String {
    let version = match &ctx.store.cloud.info {
        Some(Ok(info)) => format!("ferrocloud {} · ", info.version),
        _ => String::new(),
    };
    let data = if labels.keep_data {
        format!("data is kept between starts, in the volume {CLOUD_VOLUME}")
    } else {
        "data isn't kept: it starts empty each time".to_owned()
    };
    format!("{version}{data}")
}

fn section(ui: &mut egui::Ui, p: &Palette, title: &str) {
    ui.label(
        RichText::new(title)
            .font(egui::FontId::new(11.5, crate::theme::semibold()))
            .color(p.muted),
    );
    ui.add_space(6.0);
}

/// A heading with a chevron that folds what's under it; true when clicked.
fn fold(ui: &mut egui::Ui, p: &Palette, title: &str, open: bool) -> bool {
    ui.horizontal(|ui| {
        let (rect, chevron) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::click());
        let icon = if open {
            Icon::ChevronDown
        } else {
            Icon::ChevronRight
        };
        icons::paint(ui.painter(), rect, icon, p.muted);
        let text = RichText::new(title)
            .font(egui::FontId::new(13.0, crate::theme::semibold()))
            .color(p.text);
        let label = ui.add(egui::Label::new(text).sense(egui::Sense::click()));
        (chevron | label)
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .clicked()
    })
    .inner
}
