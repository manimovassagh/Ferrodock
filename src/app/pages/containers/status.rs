//! How a container's state is shown: colours and the status pill.

use eframe::egui::{self, Color32, RichText};

use crate::app::components::table;
use crate::app::components::widgets::{self, Mark};
use crate::docker::ContainerInfo;
use crate::theme::Palette;

/// "● Running" style pill for the container's state.
pub(super) fn pill(ui: &mut egui::Ui, p: &Palette, c: &ContainerInfo) {
    widgets::pill_with(
        ui,
        &capitalize(&c.state),
        color(p, &c.state),
        mark(&c.state),
    );
}

/// Table cell: the pill with Docker's status text ("Up 5 minutes") below.
pub(super) fn cell(ui: &mut egui::Ui, p: &Palette, c: &ContainerInfo) {
    if table::is_compact(ui) {
        pill(ui, p, c);
        return;
    }
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

/// Running is filled, stopped is hollow, paused is a square, dead a
/// triangle, so states can be told apart without colour.
fn mark(state: &str) -> Mark {
    match state {
        "running" | "restarting" => Mark::Dot,
        "paused" => Mark::Square,
        "dead" => Mark::Triangle,
        _ => Mark::Ring,
    }
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
