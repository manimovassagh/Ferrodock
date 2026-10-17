//! Start / Stop / Restart controls.

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

/// Start/Stop and Restart buttons, as icons or with labels. Shows a spinner
/// while an action is in flight. Returns the clicked action, if any.
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
    let toggle = if c.is_active() {
        (Action::Stop, "■", "Stop")
    } else {
        (Action::Start, "▶", "Start")
    };
    let mut clicked = None;
    for (action, icon, label) in [toggle, (Action::Restart, "⟳", "Restart")] {
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
