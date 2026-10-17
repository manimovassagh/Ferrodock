//! How a container's state is shown: colours and the status pill.

use eframe::egui::{self, Color32, RichText};

use crate::app::components::table;
use crate::app::components::widgets::{self, Mark};
use crate::docker::{Action, ContainerInfo};
use crate::theme::Palette;

/// "● Running" style pill for the container's state. While an action is
/// in flight it says what's happening instead, e.g. "Stopping…".
pub(super) fn pill(ui: &mut egui::Ui, p: &Palette, c: &ContainerInfo, pending: Option<Action>) {
    if let Some(label) = pending.and_then(transition) {
        widgets::pill_with(ui, label, p.warning, Mark::Dot);
        return;
    }
    widgets::pill_with(
        ui,
        &capitalize(&c.state),
        color(p, &c.state),
        mark(&c.state),
    );
}

/// What a container is doing while `action` runs.
fn transition(action: Action) -> Option<&'static str> {
    Some(match action {
        Action::Start => "Starting…",
        Action::Stop => "Stopping…",
        Action::Restart => "Restarting…",
        Action::Pause => "Pausing…",
        Action::Unpause => "Resuming…",
        Action::Remove(_) => "Removing…",
        _ => return None,
    })
}

/// Table cell: the pill with Docker's status text ("Up 5 minutes") below.
pub(super) fn cell(ui: &mut egui::Ui, p: &Palette, c: &ContainerInfo, pending: Option<Action>) {
    if table::is_compact(ui) {
        pill(ui, p, c, pending);
        return;
    }
    ui.vertical(|ui| {
        ui.add_space(6.0);
        pill(ui, p, c, pending);
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

pub(super) fn color(p: &Palette, state: &str) -> Color32 {
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
