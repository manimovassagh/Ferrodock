//! How a container's state is shown: colours and the status pill.

use eframe::egui::{self, Color32, RichText};

use crate::app::components::widgets;
use crate::docker::ContainerInfo;
use crate::theme::Palette;

/// "● Running" style pill for the container's state.
pub(super) fn pill(ui: &mut egui::Ui, p: &Palette, c: &ContainerInfo) {
    widgets::pill(ui, &capitalize(&c.state), color(p, &c.state));
}

/// Table cell: the pill with Docker's status text ("Up 5 minutes") below.
pub(super) fn cell(ui: &mut egui::Ui, p: &Palette, c: &ContainerInfo) {
    ui.vertical(|ui| {
        ui.add_space(6.0);
        pill(ui, p, c);
        // Docker's status is just "Created" for new containers, which the
        // pill already says.
        if !c.status.eq_ignore_ascii_case(&c.state) {
            ui.add(
                egui::Label::new(RichText::new(&c.status).size(12.0).color(p.muted))
                    .truncate()
                    .selectable(false),
            );
        }
    });
}

fn color(p: &Palette, state: &str) -> Color32 {
    match state {
        "running" => p.success,
        "paused" | "restarting" => p.warning,
        "dead" => p.danger,
        _ => p.muted,
    }
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}
