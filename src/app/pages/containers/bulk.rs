//! The bar shown while containers are ticked: what's selected, and the
//! actions that apply to all of them.

use eframe::egui;

use super::{ContainersPage, actions};
use crate::app::components::icons::Icon;
use crate::app::components::widgets;
use crate::app::page::{ConfirmOption, Confirmation, Intent, PageCtx};
use crate::app::pages::remove_task;
use crate::docker::{Action, ContainerInfo, Resource, Task, plural};

/// How many names the removal question lists before "and n more".
const LISTED: usize = 6;

/// "3 selected · 1 running" with Start, Stop, Restart and Remove.
pub(super) fn bar(
    page: &mut ContainersPage,
    ui: &mut egui::Ui,
    ctx: &mut PageCtx,
    visible: &[&ContainerInfo],
) {
    let p = ctx.palette;
    let chosen: Vec<&ContainerInfo> = visible
        .iter()
        .copied()
        .filter(|c| page.marked.contains(&c.id))
        .collect();
    let running = chosen.iter().filter(|c| c.is_active()).count();
    egui::Frame::new()
        .fill(p.accent.gamma_multiply(0.08))
        .stroke(egui::Stroke::new(1.0, p.accent.gamma_multiply(0.3)))
        .corner_radius(egui::CornerRadius::same(10))
        .inner_margin(egui::Margin::symmetric(12, 6))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!("{} selected", chosen.len()))
                        .font(egui::FontId::new(13.5, crate::theme::semibold()))
                        .color(p.text),
                );
                if running > 0 {
                    ui.label(
                        egui::RichText::new(format!("· {running} running"))
                            .size(13.0)
                            .color(p.muted),
                    );
                }
                ui.add_space(8.0);
                let groups = [
                    (Action::Start, Icon::Play, "Start"),
                    (Action::Stop, Icon::Stop, "Stop"),
                    (Action::Restart, Icon::Restart, "Restart"),
                ];
                for (action, icon, label) in groups {
                    let targets: Vec<&ContainerInfo> = chosen
                        .iter()
                        .copied()
                        .filter(|c| match action {
                            Action::Start => !c.is_active(),
                            Action::Stop => c.is_active(),
                            _ => true,
                        })
                        .collect();
                    let response = ui
                        .add_enabled_ui(!targets.is_empty(), |ui| widgets::button(ui, icon, label))
                        .inner;
                    if response.clicked() {
                        for c in targets {
                            ctx.request(actions::intent(action, c));
                        }
                    }
                }
                let remove = widgets::danger_button(ui, p, &format!("Remove {}…", chosen.len()));
                if remove.clicked() && !chosen.is_empty() {
                    ctx.request(confirm_removal(&chosen));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if widgets::icon_button(ui, Icon::Close, "Clear the selection").clicked() {
                        page.marked.clear();
                    }
                });
            });
        });
    ui.add_space(6.0);
}

/// "Remove 3 containers?", listing them. When some run, a box (ticked)
/// stops those first; unticked, only the stopped ones go.
pub(super) fn confirm_removal(chosen: &[&ContainerInfo]) -> Intent {
    let running = chosen.iter().filter(|c| c.is_active()).count();
    let mut names: Vec<String> = chosen
        .iter()
        .take(LISTED)
        .map(|c| {
            if c.is_active() {
                format!("{} (running)", c.name)
            } else {
                c.name.clone()
            }
        })
        .collect();
    if chosen.len() > LISTED {
        names.push(format!("and {} more", chosen.len() - LISTED));
    }
    let stopped: Vec<Task> = chosen
        .iter()
        .filter(|c| !c.is_active())
        .map(|c| remove_task(Resource::Containers, &c.id, &c.name))
        .collect();
    let option = (running > 0).then(|| ConfirmOption {
        label: "Stop running ones first".into(),
        hint: format!(
            "{} running. Without this, {} skipped and kept.",
            if running == 1 {
                "1 of them is".to_owned()
            } else {
                format!("{running} of them are")
            },
            if running == 1 { "it is" } else { "they are" }
        ),
        on: true,
        tasks: chosen
            .iter()
            .map(|c| {
                if c.is_active() {
                    Task {
                        action: Action::StopAndRemove,
                        id: c.id.clone(),
                        name: c.name.clone(),
                    }
                } else {
                    remove_task(Resource::Containers, &c.id, &c.name)
                }
            })
            .collect(),
    });
    Intent::Confirm(Confirmation {
        title: format!("Remove {}?", plural(chosen.len(), "container")),
        message: format!(
            "{}.\n\nTheir files go with them. Images, volumes and networks are kept.",
            names.join(", ")
        ),
        action_label: format!("Remove {}", chosen.len()),
        tasks: stopped,
        intents: Vec::new(),
        option,
        typed: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn container(name: &str, running: bool) -> ContainerInfo {
        ContainerInfo {
            id: name.into(),
            name: name.into(),
            state: if running { "running" } else { "exited" }.into(),
            ..Default::default()
        }
    }

    #[test]
    fn removal_stops_running_ones_only_when_ticked() {
        let all = [container("api", true), container("db", false)];
        let chosen: Vec<&ContainerInfo> = all.iter().collect();
        let Intent::Confirm(mut c) = confirm_removal(&chosen) else {
            panic!("expected a confirmation");
        };
        assert_eq!(c.title, "Remove 2 containers?");
        assert!(c.message.starts_with("api (running), db."));
        let actions = |tasks: Vec<Task>| tasks.iter().map(|t| t.action).collect::<Vec<_>>();
        assert_eq!(
            actions(c.clone().chosen_tasks()),
            [Action::StopAndRemove, Action::Remove(Resource::Containers)]
        );
        c.option.as_mut().unwrap().on = false;
        assert_eq!(
            actions(c.chosen_tasks()),
            [Action::Remove(Resource::Containers)]
        );
    }

    #[test]
    fn no_option_when_nothing_runs() {
        let all = [container("db", false)];
        let Intent::Confirm(c) = confirm_removal(&all.iter().collect::<Vec<_>>()) else {
            panic!("expected a confirmation");
        };
        assert!(c.option.is_none());
        assert_eq!(c.title, "Remove 1 container?");
    }
}
