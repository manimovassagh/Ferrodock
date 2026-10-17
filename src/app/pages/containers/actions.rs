//! Start / Stop / Pause / Restart controls.

use eframe::egui;

use crate::app::components::widgets;
use crate::app::page::Intent;
use crate::docker::{Action, ContainerInfo, Task};

pub(super) fn intent(action: Action, c: &ContainerInfo) -> Intent {
    Intent::Run(Task {
        action,
        id: c.id.clone(),
        name: c.name.clone(),
    })
}

/// The lifecycle actions that apply to `c` right now, with their icon and
/// label: start or stop, pause or resume (when it applies), restart.
pub(super) fn available(c: &ContainerInfo) -> Vec<(Action, &'static str, &'static str)> {
    let mut list = vec![if c.is_active() {
        (Action::Stop, "■", "Stop")
    } else {
        (Action::Start, "▶", "Start")
    }];
    if c.is_paused() {
        list.push((Action::Unpause, "⏵", "Resume"));
    } else if c.can_pause() {
        list.push((Action::Pause, "⏸", "Pause"));
    }
    list.push((Action::Restart, "⟳", "Restart"));
    list
}

/// Start/Stop, Pause/Resume and Restart buttons, as icons or with labels.
/// Shows a spinner while an action is in flight. Returns the clicked
/// action, if any.
pub(super) fn buttons(
    ui: &mut egui::Ui,
    c: &ContainerInfo,
    busy: bool,
    with_labels: bool,
) -> Option<Action> {
    if busy {
        ui.spinner();
        return None;
    }
    let mut clicked = None;
    for (action, icon, label) in available(c) {
        let response = if with_labels {
            ui.button(format!("{icon}  {label}"))
        } else {
            widgets::icon_button(ui, icon, label)
        };
        if response.clicked() {
            clicked = Some(action);
        }
    }
    clicked
}
