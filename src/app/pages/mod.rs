//! The pages listed in the sidebar. To add one, implement [`Page`] in a new
//! file and register it in [`all`].

mod compose;
mod containers;
mod disk;
mod images;
mod networks;
mod volumes;

use eframe::egui;

use super::components::{inspect, widgets};
use super::page::{Confirmation, Intent, Page, PageCtx};
use crate::docker::{Action, Resource, Task, plural};
use crate::theme::Palette;

/// Every page, in sidebar order.
pub fn all() -> Vec<Box<dyn Page>> {
    vec![
        Box::new(containers::ContainersPage::new()),
        Box::new(compose::ComposePage::default()),
        Box::new(images::ImagesPage::default()),
        Box::new(volumes::VolumesPage::default()),
        Box::new(networks::NetworksPage::default()),
        Box::new(disk::DiskPage::default()),
    ]
}

/// Collapsible Inspect section at the end of a details pane. The inspect
/// output is loaded the first time it's opened.
fn inspect_section(ui: &mut egui::Ui, ctx: &mut PageCtx, resource: Resource, id: &str) {
    let p = ctx.palette;
    egui::CollapsingHeader::new(egui::RichText::new("INSPECT").size(11.0).color(p.muted))
        .id_salt(("inspect", id))
        .show(ui, |ui| {
            if inspect::needs_loading(ctx.store, id) {
                ctx.request(Intent::Inspect(resource, id.to_owned()));
            }
            inspect::show(ui, p, ctx.store, id);
        });
}

/// "In use" / "Unused" badge for images and volumes.
fn usage_pill(ui: &mut egui::Ui, p: &Palette, users: usize) {
    if users > 0 {
        widgets::pill(ui, "In use", p.success);
    } else {
        widgets::pill(ui, "Unused", p.muted);
    }
}

fn remove_task(resource: Resource, id: &str, name: &str) -> Task {
    Task {
        action: Action::Remove(resource),
        id: id.to_owned(),
        name: name.to_owned(),
    }
}

/// The details-pane Remove button. When `blocked` holds a reason the button
/// is disabled and the reason shows on hover. Asks before removing.
fn remove_button(
    ui: &mut egui::Ui,
    ctx: &mut PageCtx,
    kind: &str,
    task: Task,
    blocked: Option<&str>,
) {
    let p = ctx.palette;
    if let Some(reason) = blocked {
        ui.add_enabled(false, egui::Button::new("Remove"))
            .on_disabled_hover_text(reason);
    } else if widgets::danger_button(ui, p, "Remove").clicked() {
        ctx.request(confirm_removal(kind, vec![task]));
    }
}

/// Header button that removes every unused item of a kind, after asking.
fn remove_unused_button(ui: &mut egui::Ui, ctx: &mut PageCtx, kind: &str, tasks: Vec<Task>) {
    let hint = if tasks.is_empty() {
        format!("No unused {kind}s")
    } else {
        format!("Remove {}", plural(tasks.len(), &format!("unused {kind}")))
    };
    let button = ui
        .add_enabled(!tasks.is_empty(), egui::Button::new("Remove unused"))
        .on_hover_text(&hint)
        .on_disabled_hover_text(&hint);
    if button.clicked() {
        ctx.request(confirm_removal(kind, tasks));
    }
}

/// Builds the "are you sure?" question for removing `tasks`.
/// `kind` is the singular noun, e.g. "volume".
fn confirm_removal(kind: &str, tasks: Vec<Task>) -> Intent {
    const LISTED: usize = 6;
    let (title, message) = match tasks.as_slice() {
        [one] => (
            format!("Remove {kind} “{}”?", one.name),
            format!("The {kind} is deleted permanently. This can't be undone."),
        ),
        many => {
            let mut names: Vec<&str> = many.iter().take(LISTED).map(|t| t.name.as_str()).collect();
            let rest = many.len().saturating_sub(LISTED);
            let more = format!("and {rest} more");
            if rest > 0 {
                names.push(&more);
            }
            (
                format!("Remove {}?", plural(many.len(), &format!("unused {kind}"))),
                format!(
                    "These are deleted permanently: {}. This can't be undone.",
                    names.join(", ")
                ),
            )
        }
    };
    Intent::Confirm(Confirmation {
        title,
        message,
        action_label: "Remove".into(),
        tasks,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tasks(n: usize) -> Vec<Task> {
        (0..n)
            .map(|i| remove_task(Resource::Volumes, &format!("v{i}"), &format!("v{i}")))
            .collect()
    }

    #[test]
    fn single_removal_names_the_item() {
        let Intent::Confirm(c) = confirm_removal("volume", tasks(1)) else {
            panic!("expected a confirmation");
        };
        assert_eq!(c.title, "Remove volume “v0”?");
        assert_eq!(c.tasks.len(), 1);
    }

    #[test]
    fn bulk_removal_lists_a_few_names() {
        let Intent::Confirm(c) = confirm_removal("volume", tasks(8)) else {
            panic!("expected a confirmation");
        };
        assert_eq!(c.title, "Remove 8 unused volumes?");
        assert!(c.message.contains("v5, and 2 more"));
        assert_eq!(c.tasks.len(), 8);
    }
}
