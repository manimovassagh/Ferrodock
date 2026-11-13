//! What the running emulator supports: its services, each unfolding to
//! the operations it answers.

use std::collections::HashSet;

use eframe::egui::{self, RichText};

use crate::app::components::icons::{self, Icon};
use crate::app::page::PageCtx;
use crate::docker::cloud::CloudService;
use crate::docker::plural;

/// The operations of `service` that match the search; all of them when
/// its name does.
fn matching<'a>(ctx: &PageCtx, service: &'a CloudService) -> Vec<&'a str> {
    let by_name = ctx.matches_search(&[&service.title, &service.name]);
    service
        .operations
        .iter()
        .map(String::as_str)
        .filter(|operation| by_name || ctx.matches_search(&[operation]))
        .collect()
}

pub fn show(ui: &mut egui::Ui, ctx: &mut PageCtx, unfolded: &mut HashSet<String>) {
    let p = ctx.palette;
    let info = match &ctx.store.cloud.info {
        None => {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(RichText::new("Asking the emulator…").color(p.muted));
            });
            return;
        }
        Some(Err(error)) => {
            ui.label(RichText::new(error).color(p.danger));
            return;
        }
        Some(Ok(info)) => info,
    };
    let searching = !ctx.search_is_empty();
    let mut shown = 0;
    for service in &info.services {
        let operations = matching(ctx, service);
        if operations.is_empty() {
            continue;
        }
        shown += 1;
        // A search unfolds what it found.
        let open = searching || unfolded.contains(&service.name);
        let header = egui::Frame::new()
            .fill(p.bg)
            .stroke(egui::Stroke::new(1.0, p.border))
            .corner_radius(egui::CornerRadius::same(10))
            .inner_margin(egui::Margin::symmetric(12, 9))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
                    let chevron = if open {
                        Icon::ChevronDown
                    } else {
                        Icon::ChevronRight
                    };
                    icons::paint(ui.painter(), rect, chevron, p.muted);
                    ui.label(RichText::new(&service.title).strong().color(p.text));
                    ui.label(
                        RichText::new(&service.name)
                            .monospace()
                            .size(12.0)
                            .color(p.muted),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let count = if operations.len() == service.operations.len() {
                            plural(operations.len(), "operation")
                        } else {
                            format!(
                                "{} of {}",
                                operations.len(),
                                plural(service.operations.len(), "operation")
                            )
                        };
                        ui.label(RichText::new(count).size(12.5).color(p.muted));
                    });
                });
                if open {
                    ui.add_space(6.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(14.0, 4.0);
                        for operation in &operations {
                            ui.label(
                                RichText::new(*operation)
                                    .monospace()
                                    .size(12.0)
                                    .color(p.text),
                            );
                        }
                    });
                }
            })
            .response;
        let header = header.interact(egui::Sense::click());
        if header
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .clicked()
            && !searching
            && !unfolded.remove(&service.name)
        {
            unfolded.insert(service.name.clone());
        }
        ui.add_space(6.0);
    }
    if shown == 0 {
        ui.label(RichText::new("No service or operation matches.").color(p.muted));
    } else if !searching {
        ui.label(
            RichText::new(
                "Nothing is checked against policies or billed: every request is allowed. \
                 More services arrive with each version.",
            )
            .size(12.0)
            .color(p.muted),
        );
    }
}
