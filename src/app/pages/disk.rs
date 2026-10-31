//! Disk page: where the engine's disk space goes, and a clean-up that
//! shows exactly what it will delete before it runs.

use std::collections::BTreeSet;

use eframe::egui::{self, CornerRadius, RichText, Sense, Stroke, vec2};

use crate::app::components::icons::Icon;
use crate::app::components::widgets;
use crate::app::page::{Command, ConfirmOption, Confirmation, Intent, Page, PageCtx, TypedCheck};
use crate::app::store::Store;
use crate::docker::{
    Action, DiskUsage, PruneKind, RESET_WORD, Resource, Task, Usage, format_size, plural,
};
use crate::theme::Palette;

/// How many names the preview lists per kind before "and n more".
const PREVIEW: usize = 6;

#[derive(Default)]
pub struct DiskPage {
    /// What the user ticked for clean-up.
    chosen: BTreeSet<Choice>,
    /// For images: every unused image, or only untagged ones.
    all_images: bool,
}

/// The clean-up checkboxes. Images are one box; whether it means dangling
/// or all unused images is a separate toggle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Choice {
    Containers,
    Images,
    Volumes,
    Networks,
    BuildCache,
}

impl DiskPage {
    fn kind(&self, choice: Choice) -> PruneKind {
        match choice {
            Choice::Containers => PruneKind::StoppedContainers,
            Choice::Images if self.all_images => PruneKind::UnusedImages,
            Choice::Images => PruneKind::DanglingImages,
            Choice::Volumes => PruneKind::UnusedVolumes,
            Choice::Networks => PruneKind::UnusedNetworks,
            Choice::BuildCache => PruneKind::BuildCache,
        }
    }
}

/// What a clean-up of `kind` would delete, by name, from what's loaded.
/// Build cache entries aren't listed individually.
fn doomed(store: &Store, kind: PruneKind) -> Vec<&str> {
    match kind {
        PruneKind::StoppedContainers => store
            .containers
            .iter()
            .filter(|c| !c.is_active())
            .map(|c| c.name.as_str())
            .collect(),
        PruneKind::DanglingImages | PruneKind::UnusedImages => store
            .images
            .iter()
            .filter(|i| store.containers_using_image(&i.id).is_empty())
            .filter(|i| kind == PruneKind::UnusedImages || i.tags.is_empty())
            .map(|i| {
                if i.tags.is_empty() {
                    i.short_id()
                } else {
                    i.name()
                }
            })
            .collect(),
        PruneKind::UnusedVolumes => store
            .volumes
            .iter()
            .filter(|v| store.containers_using_volume(&v.name).is_empty())
            .map(|v| v.name.as_str())
            .collect(),
        PruneKind::UnusedNetworks => store
            .networks
            .iter()
            .filter(|n| !n.is_builtin() && store.containers_on_network(&n.name).is_empty())
            .map(|n| n.name.as_str())
            .collect(),
        PruneKind::BuildCache => Vec::new(),
    }
}

/// Bytes a clean-up of `kind` would free, when the engine says.
fn freeable(store: &Store, disk: &DiskUsage, kind: PruneKind) -> Option<i64> {
    match kind {
        PruneKind::StoppedContainers => disk.containers.as_ref().map(|u| u.reclaimable),
        PruneKind::UnusedImages => disk.images.as_ref().map(|u| u.reclaimable),
        PruneKind::DanglingImages => Some(
            store
                .images
                .iter()
                .filter(|i| i.tags.is_empty() && store.containers_using_image(&i.id).is_empty())
                .map(|i| i.size)
                .sum(),
        ),
        PruneKind::UnusedVolumes => disk.volumes.as_ref().map(|u| u.reclaimable),
        PruneKind::UnusedNetworks => None,
        PruneKind::BuildCache => disk.build_cache.as_ref().map(|u| u.reclaimable),
    }
}

fn prune_task(kind: PruneKind) -> Task {
    Task {
        action: Action::Prune(kind),
        id: format!("prune:{kind:?}"),
        name: kind.noun().to_owned(),
    }
}

/// The "are you sure?" question for cleaning up `kinds`.
fn confirm_cleanup(store: &Store, kinds: &[PruneKind]) -> Intent {
    let lines: Vec<String> = kinds
        .iter()
        .map(|&kind| {
            let names = doomed(store, kind);
            if kind == PruneKind::BuildCache || names.is_empty() {
                format!("• All {}", kind.noun())
            } else {
                let rest = names.len().saturating_sub(PREVIEW);
                let mut shown = names
                    .into_iter()
                    .take(PREVIEW)
                    .collect::<Vec<_>>()
                    .join(", ");
                if rest > 0 {
                    shown.push_str(&format!(" and {rest} more"));
                }
                format!("• {}: {shown}", capitalize(kind.noun()))
            }
        })
        .collect();
    let volumes = kinds.contains(&PruneKind::UnusedVolumes);
    Intent::Confirm(Confirmation {
        title: "Clean up?".into(),
        message: format!(
            "This deletes:\n{}\n\n{}This can't be undone.",
            lines.join("\n"),
            if volumes {
                "The data in those volumes is lost. "
            } else {
                ""
            }
        ),
        action_label: "Clean up".into(),
        tasks: kinds.iter().copied().map(prune_task).collect(),
        ..Default::default()
    })
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

impl Page for DiskPage {
    fn title(&self) -> &'static str {
        "Disk"
    }

    fn icon(&self) -> Icon {
        Icon::HardDrive
    }

    fn resource(&self) -> Resource {
        Resource::Disk
    }

    fn badge(&self, store: &Store) -> String {
        store
            .disk
            .as_ref()
            .filter(|d| d.is_reported())
            .map(|d| format_size(d.total_size()))
            .unwrap_or_default()
    }

    fn summary(&self, store: &Store) -> String {
        match &store.disk {
            None => "Loading…".into(),
            Some(d) if !d.is_reported() => {
                "This Docker Engine doesn't report disk usage (needs Engine 29 or newer)".into()
            }
            Some(d) => format!(
                "{} used · {} can be freed",
                format_size(d.total_size()),
                format_size(d.reclaimable())
            ),
        }
    }

    fn show(&mut self, ui: &mut egui::Ui, ctx: &mut PageCtx) {
        let p = ctx.palette;
        let store = ctx.store;
        let Some(disk) = &store.disk else {
            ui.add_space(40.0);
            ui.vertical_centered(|ui| ui.spinner());
            return;
        };

        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                if disk.is_reported() {
                    ui.columns(4, |cols| {
                        let parts = [
                            ("Images", &disk.images, "image"),
                            ("Containers", &disk.containers, "container"),
                            ("Volumes", &disk.volumes, "volume"),
                            ("Build cache", &disk.build_cache, "item"),
                        ];
                        for (ui, (title, usage, noun)) in cols.iter_mut().zip(parts) {
                            card(ui, p, title, usage.as_ref(), noun);
                        }
                    });
                    ui.add_space(22.0);
                }
                self.cleanup(ui, ctx, disk);
                ui.add_space(22.0);
                danger_zone(ui, ctx);
            });
    }

    fn has_selection(&self, _store: &Store) -> bool {
        false
    }

    fn show_details(&mut self, _ui: &mut egui::Ui, _ctx: &mut PageCtx) {}

    fn clear_selection(&mut self) {}

    fn select(&mut self, _id: &str) {}

    fn commands(&self, store: &Store) -> Vec<Command> {
        [
            PruneKind::StoppedContainers,
            PruneKind::DanglingImages,
            PruneKind::UnusedImages,
            PruneKind::UnusedVolumes,
            PruneKind::UnusedNetworks,
            PruneKind::BuildCache,
        ]
        .into_iter()
        .map(|kind| {
            Command::intent(
                format!("Clean up {}", kind.noun()),
                "Disk",
                confirm_cleanup(store, &[kind]),
            )
        })
        .chain(std::iter::once(Command::intent(
            "Reset Docker…",
            "Disk · removes everything",
            confirm_reset(store),
        )))
        .collect()
    }
}

impl DiskPage {
    fn cleanup(&mut self, ui: &mut egui::Ui, ctx: &mut PageCtx, disk: &DiskUsage) {
        let p = ctx.palette;
        let store = ctx.store;
        ui.label(RichText::new("CLEAN UP").size(11.0).color(p.muted));
        ui.add_space(4.0);
        egui::Frame::new()
            .fill(p.bg)
            .stroke(Stroke::new(1.0, p.border))
            .corner_radius(CornerRadius::same(10))
            .inner_margin(egui::Margin::same(14))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                egui::Grid::new("cleanup")
                    .num_columns(3)
                    .spacing([18.0, 10.0])
                    .show(ui, |ui| {
                        for (choice, label) in [
                            (Choice::Containers, "Stopped containers"),
                            (Choice::Images, "Unused images"),
                            (Choice::Volumes, "Unused volumes"),
                            (Choice::Networks, "Unused networks"),
                            (Choice::BuildCache, "Build cache"),
                        ] {
                            let kind = self.kind(choice);
                            let mut on = self.chosen.contains(&choice);
                            if ui.checkbox(&mut on, label).changed() {
                                if on {
                                    self.chosen.insert(choice);
                                } else {
                                    self.chosen.remove(&choice);
                                }
                            }
                            let count = doomed(store, kind).len();
                            let size = freeable(store, disk, kind)
                                .filter(|b| *b > 0)
                                .map(format_size);
                            let what = match (choice, size) {
                                (Choice::BuildCache, Some(size)) => size,
                                (Choice::BuildCache, None) => "—".into(),
                                (_, Some(size)) => format!("{count} · {size}"),
                                (_, None) => count.to_string(),
                            };
                            ui.label(RichText::new(what).color(p.muted));
                            match choice {
                                // Each says what it frees: the Images card
                                // counts all unused images, so "Untagged
                                // only" freeing nothing would otherwise
                                // look like a contradiction.
                                Choice::Images => {
                                    let label = |name: &str, kind| match freeable(store, disk, kind)
                                    {
                                        Some(bytes) if bytes > 0 => {
                                            format!("{name} ({})", format_size(bytes))
                                        }
                                        _ => format!("{name} (nothing)"),
                                    };
                                    let untagged =
                                        label("Untagged only", PruneKind::DanglingImages);
                                    let unused = label("All unused", PruneKind::UnusedImages);
                                    ui.horizontal(|ui| {
                                        ui.radio_value(&mut self.all_images, false, untagged);
                                        ui.radio_value(&mut self.all_images, true, unused);
                                    });
                                }
                                Choice::Volumes => {
                                    ui.label(
                                        RichText::new("Deletes the data in them")
                                            .size(12.0)
                                            .color(p.warning),
                                    );
                                }
                                _ => {
                                    ui.label("");
                                }
                            }
                            ui.end_row();
                        }
                    });

                ui.add_space(12.0);
                let kinds: Vec<PruneKind> = self.chosen.iter().map(|&c| self.kind(c)).collect();
                let busy = kinds
                    .iter()
                    .any(|&k| store.pending.contains_key(&prune_task(k).id));
                ui.horizontal(|ui| {
                    if busy {
                        ui.spinner();
                        ui.label(RichText::new("Cleaning up…").color(p.muted));
                    } else if widgets::danger_button(ui, p, "Clean up…").clicked()
                        && !kinds.is_empty()
                    {
                        ctx.request(confirm_cleanup(store, &kinds));
                    }
                    if kinds.is_empty() && !busy {
                        ui.label(RichText::new("Tick what to remove").color(p.muted));
                    }
                });
            });
    }
}

/// The Reset Docker section, apart from Clean up so it's hard to hit by
/// accident.
fn danger_zone(ui: &mut egui::Ui, ctx: &mut PageCtx) {
    let p = ctx.palette;
    let store = ctx.store;
    ui.label(RichText::new("DANGER ZONE").size(11.0).color(p.danger));
    ui.add_space(4.0);
    egui::Frame::new()
        .fill(p.danger.gamma_multiply(0.06))
        .stroke(Stroke::new(1.0, p.danger.gamma_multiply(0.45)))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(egui::Margin::same(14))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let text_width = (ui.available_width() - 140.0).max(160.0);
                ui.vertical(|ui| {
                    ui.set_max_width(text_width);
                    ui.label(
                        RichText::new("Reset Docker")
                            .font(egui::FontId::new(14.5, crate::theme::semibold()))
                            .color(p.text),
                    );
                    ui.label(
                        RichText::new(
                            "Removes everything, in use or not: all containers, images, \
                             volumes, custom networks and the build cache. For a clean start.",
                        )
                        .color(p.muted),
                    );
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let busy = store.pending.contains_key(RESET_ID);
                    if busy {
                        ui.label(RichText::new("Resetting…").color(p.muted));
                        ui.spinner();
                    } else if widgets::danger_button(ui, p, "Reset Docker…").clicked() {
                        ctx.request(confirm_reset(store));
                    }
                });
            });
        });
}

const RESET_ID: &str = "reset";

fn reset_task(keep_volumes: bool) -> Task {
    Task {
        action: Action::Reset { keep_volumes },
        id: RESET_ID.into(),
        name: "Docker".into(),
    }
}

/// "Reset Docker?" with what goes, a Keep volumes box, and the word to
/// type.
fn confirm_reset(store: &Store) -> Intent {
    let size = |usage: Option<&Usage>| {
        usage
            .map(|u| format!(" · {}", format_size(u.total_size)))
            .filter(|_| usage.is_some_and(|u| u.total_size > 0))
            .unwrap_or_default()
    };
    let disk = store.disk.as_ref();
    let running = store.running_count();
    let custom_networks = store.networks.iter().filter(|n| !n.is_builtin()).count();
    let mut lines = vec![format!(
        "• {}{}{}",
        plural(store.containers.len(), "container"),
        if running > 0 {
            format!(" ({running} running, stopped first)")
        } else {
            String::new()
        },
        size(disk.and_then(|d| d.containers.as_ref()))
    )];
    lines.push(format!(
        "• {}{}",
        plural(store.images.len(), "image"),
        size(disk.and_then(|d| d.images.as_ref()))
    ));
    lines.push(format!(
        "• {}{} (deletes their data)",
        plural(store.volumes.len(), "volume"),
        size(disk.and_then(|d| d.volumes.as_ref()))
    ));
    lines.push(format!("• {}", plural(custom_networks, "custom network")));
    lines.push(format!(
        "• Build cache{}",
        size(disk.and_then(|d| d.build_cache.as_ref()))
    ));
    Intent::Confirm(Confirmation {
        title: "Reset Docker?".into(),
        message: format!(
            "This removes everything below from this engine. It can't be undone.\n\n{}\n\n\
             Built-in networks (bridge, host, none) are never touched.",
            lines.join("\n")
        ),
        action_label: "Reset Docker".into(),
        tasks: vec![reset_task(false)],
        intents: Vec::new(),
        option: Some(ConfirmOption {
            label: "Keep volumes".into(),
            hint: "Reset everything else, keep your data".into(),
            on: false,
            tasks: vec![reset_task(true)],
        }),
        typed: Some(TypedCheck::new(RESET_WORD)),
    })
}

/// One usage summary: size, a bar of how much could be freed, counts.
fn card(ui: &mut egui::Ui, p: &Palette, title: &str, usage: Option<&Usage>, noun: &str) {
    egui::Frame::new()
        .fill(p.bg)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(egui::Margin::same(14))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(
                RichText::new(title.to_uppercase())
                    .size(11.0)
                    .color(p.muted),
            );
            let Some(u) = usage else {
                ui.label(RichText::new("—").size(22.0).color(p.text));
                return;
            };
            ui.label(
                RichText::new(format_size(u.total_size))
                    .size(22.0)
                    .strong()
                    .color(p.text),
            );
            ui.add_space(6.0);
            bar(ui, p, u.reclaimable, u.total_size);
            ui.add_space(6.0);
            ui.label(
                RichText::new(format!("{} can be freed", format_size(u.reclaimable)))
                    .size(12.0)
                    .color(p.muted),
            );
            ui.label(
                RichText::new(format!(
                    "{} · {} in use",
                    plural(u.count as usize, noun),
                    u.active
                ))
                .size(12.0)
                .color(p.muted),
            );
        });
}

/// A thin bar: the freeable share of `total`, in the theme's data colour
/// (the accent reads as a warning, which freeable space isn't).
fn bar(ui: &mut egui::Ui, p: &Palette, part: i64, total: i64) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 6.0), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(3), p.border);
    if total > 0 && part > 0 {
        let share = (part as f32 / total as f32).clamp(0.0, 1.0);
        let mut filled = rect;
        filled.set_width((rect.width() * share).max(4.0));
        painter.rect_filled(filled, CornerRadius::same(3), p.glow);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confirmation_lists_what_goes() {
        let store = Store::new();
        let Intent::Confirm(c) =
            confirm_cleanup(&store, &[PruneKind::BuildCache, PruneKind::UnusedVolumes])
        else {
            panic!("expected a confirmation");
        };
        assert!(c.message.contains("All build cache"));
        assert!(c.message.contains("data in those volumes is lost"));
        assert_eq!(c.tasks.len(), 2);
        assert_eq!(c.tasks[0].action, Action::Prune(PruneKind::BuildCache));
    }

    #[test]
    fn reset_needs_the_word_and_can_keep_volumes() {
        let Intent::Confirm(mut c) = confirm_reset(&Store::new()) else {
            panic!("expected a confirmation");
        };
        assert!(!c.ready());
        c.typed.as_mut().unwrap().entered = "reset".into();
        assert!(c.ready());
        assert!(c.message.contains("0 volumes (deletes their data)"));
        let keep = |c: Confirmation| match c.chosen_tasks()[0].action {
            Action::Reset { keep_volumes } => keep_volumes,
            _ => panic!("expected a reset"),
        };
        assert!(!keep(c.clone()));
        c.option.as_mut().unwrap().on = true;
        assert!(keep(c));
    }
}
