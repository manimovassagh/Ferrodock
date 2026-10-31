//! Linux machines page: small Linux computers (containers of a
//! distribution, set up with your user) to work in. Your machines are
//! listed first, then a card per distribution to create one; an open
//! machine shows its shell across the page.

mod form;

use std::collections::HashSet;

use eframe::egui::{self, CornerRadius, RichText, Stroke};

use super::containers::terminal::{self, TerminalTab};
use crate::app::components::icons::Icon;
use crate::app::components::widgets;
use crate::app::page::{Command, Confirmation, Intent, Page, PageCtx};
use crate::app::store::Store;
use crate::docker::{Action, ContainerInfo, DISTROS, Distro, Resource, Task, distro};
use crate::theme::Palette;
use form::MachineForm;

/// Narrowest a card gets before the grid drops a column.
const CARD_WIDTH: f32 = 200.0;
/// What a machine's row keeps free on the right for its state and buttons.
const BUTTONS_WIDTH: f32 = 290.0;

#[derive(Default)]
pub struct MachinesPage {
    /// The Create dialog, while it's open.
    form: Option<MachineForm>,
    /// The machine whose shell fills the page (container ID).
    open: Option<String>,
    /// A machine just asked for (named), opened once it's ready.
    awaited: Option<String>,
    /// Failed creations the user closed, by name.
    dismissed: HashSet<String>,
    terminal: TerminalTab,
}

/// The containers that are machines.
fn machines(store: &Store) -> impl Iterator<Item = &ContainerInfo> {
    store.containers.iter().filter(|c| c.machine.is_some())
}

/// The distribution of a machine, and its version (`24.04`).
fn kind(c: &ContainerInfo) -> (Option<&'static Distro>, &str) {
    let id = c.machine.as_ref().map_or("", |(id, _)| id.as_str());
    let tag = c.image.rsplit_once(':').map_or("", |(_, tag)| tag);
    (distro(id), tag)
}

/// Being created: downloading, starting or being set up.
fn creating(store: &Store, name: &str) -> bool {
    store.pending.contains_key(&format!("machine:{name}")) || store.setting_up.contains(name)
}

/// A folder as people know it: `/home/me/code` → `~/code`.
fn tilde(folder: &str) -> String {
    let home = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE"));
    match home
        .ok()
        .filter(|h| !h.is_empty())
        .and_then(|h| folder.strip_prefix(&h).map(str::to_owned))
    {
        Some(rest) if rest.is_empty() || rest.starts_with(['/', '\\']) => format!("~{rest}"),
        _ => folder.to_owned(),
    }
}

/// A distribution's badge: its initial on its colour.
fn badge(ui: &mut egui::Ui, distro: &Distro, size: f32) {
    let [r, g, b] = distro.color;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    ui.painter().rect_filled(
        rect,
        CornerRadius::same((size / 4.0) as u8),
        egui::Color32::from_rgb(r, g, b),
    );
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        distro.name[..1].to_uppercase(),
        egui::FontId::new(size * 0.45, crate::theme::semibold()),
        egui::Color32::WHITE,
    );
}

fn section(ui: &mut egui::Ui, p: &Palette, title: &str) {
    ui.label(
        RichText::new(title)
            .font(egui::FontId::new(11.5, crate::theme::semibold()))
            .color(p.muted),
    );
    ui.add_space(6.0);
}

/// Asks, then stops and removes the machine.
fn confirm_remove(c: &ContainerInfo) -> Intent {
    let shared = c.machine.as_ref().is_some_and(|(_, f)| !f.is_empty());
    let mut message =
        "The machine and everything in it are deleted. This can't be undone.".to_owned();
    if shared {
        message.push_str(" The shared folder on this computer is kept.");
    }
    Intent::Confirm(Confirmation {
        title: format!("Remove the machine “{}”?", c.name),
        message,
        action_label: "Remove".into(),
        tasks: vec![task(Action::StopAndRemove, c)],
        ..Default::default()
    })
}

fn task(action: Action, c: &ContainerInfo) -> Task {
    Task {
        action,
        id: c.id.clone(),
        name: c.name.clone(),
    }
}

impl Page for MachinesPage {
    fn title(&self) -> &'static str {
        "Linux machines"
    }

    fn icon(&self) -> Icon {
        Icon::Terminal
    }

    fn resource(&self) -> Resource {
        Resource::Containers
    }

    fn badge(&self, store: &Store) -> String {
        match machines(store).filter(|c| c.is_running()).count() {
            0 => String::new(),
            running => running.to_string(),
        }
    }

    fn summary(&self, store: &Store) -> String {
        let all = machines(store).count();
        if all == 0 {
            return "Small Linux computers to work and try things in".into();
        }
        let running = machines(store).filter(|c| c.is_running()).count();
        format!(
            "{} · {running} running",
            crate::docker::plural(all, "machine")
        )
    }

    fn search_hint(&self) -> &'static str {
        "Filter machines"
    }

    fn bottom(&self) -> bool {
        true
    }

    fn has_overlay(&self) -> bool {
        self.form.is_some()
    }

    fn show(&mut self, ui: &mut egui::Ui, ctx: &mut PageCtx) {
        let p = ctx.palette;
        if let Some(open) = &mut self.form {
            match form::show(ui, p, ctx.store, open) {
                Some(form::Outcome::Create(spec)) => {
                    self.dismissed.remove(&spec.name);
                    self.awaited = Some(spec.name.clone());
                    ctx.request(Intent::CreateMachine(spec));
                    self.form = None;
                }
                Some(form::Outcome::Cancel) => self.form = None,
                None => {}
            }
        }
        // A machine just created opens once it's set up.
        if let Some(name) = &self.awaited {
            if ctx.store.machine_errors.contains_key(name) {
                self.awaited = None;
            } else if !creating(ctx.store, name)
                && let Some(c) = machines(ctx.store).find(|c| &c.name == name)
            {
                self.open = Some(c.id.clone());
                self.awaited = None;
            }
        }
        let open = self.open.as_deref().and_then(|id| ctx.store.container(id));
        match open {
            Some(c) => {
                let c = c.clone();
                self.machine_view(ui, ctx, &c);
            }
            None => {
                self.open = None;
                self.list(ui, ctx);
            }
        }
    }

    fn has_selection(&self, _store: &Store) -> bool {
        false
    }

    fn show_details(&mut self, _ui: &mut egui::Ui, _ctx: &mut PageCtx) {}

    fn clear_selection(&mut self) {
        self.open = None;
    }

    /// Opens the machine with this ID, or the Create dialog of a
    /// distribution with `new:` and its ID.
    fn select(&mut self, id: &str) {
        match id.strip_prefix("new:") {
            Some(distro_id) => {
                self.form = distro(distro_id).map(|d| MachineForm::new(d, &Store::new()));
            }
            None => self.open = Some(id.to_owned()),
        }
    }

    fn commands(&self, store: &Store) -> Vec<Command> {
        let mut list: Vec<Command> = machines(store)
            .map(|c| Command::select(format!("Open {}", c.name), "Linux machine", c.id.clone()))
            .collect();
        list.extend(DISTROS.iter().map(|d| {
            Command::select(
                format!("New {} machine…", d.name),
                "Linux machines",
                format!("new:{}", d.id),
            )
        }));
        list
    }
}

impl MachinesPage {
    /// Your machines, then the distributions to create one from.
    fn list(&mut self, ui: &mut egui::Ui, ctx: &mut PageCtx) {
        let p = ctx.palette;
        let store = ctx.store;
        let mine: Vec<&ContainerInfo> = machines(store)
            .filter(|c| ctx.matches_search(&[&c.name, &c.image]))
            .collect();
        // Being made, before their container shows up.
        let mut starting: Vec<&str> = store
            .pending
            .keys()
            .filter_map(|k| k.strip_prefix("machine:"))
            .filter(|name| !mine.iter().any(|c| c.name == *name))
            .collect();
        starting.sort();
        let mut failed: Vec<(&String, &String)> = store
            .machine_errors
            .iter()
            .filter(|(name, _)| !self.dismissed.contains(*name))
            .collect();
        failed.sort();
        let distros: Vec<&'static Distro> = DISTROS
            .iter()
            .filter(|d| ctx.matches_search(&[d.name, d.summary, d.image]))
            .collect();
        if mine.is_empty() && distros.is_empty() && starting.is_empty() {
            if widgets::empty_state(
                ui,
                p,
                Icon::Search,
                "No matches",
                "No machine or distribution matches your filter.",
                Some("Clear filter"),
            ) {
                ctx.request(Intent::ClearSearch);
            }
            return;
        }
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                if !(mine.is_empty() && starting.is_empty() && failed.is_empty()) {
                    section(ui, p, "YOUR MACHINES");
                    for name in &starting {
                        self.starting_row(ui, ctx, name);
                    }
                    for (name, message) in &failed {
                        if failed_row(ui, p, name, message) {
                            self.dismissed.insert((*name).clone());
                        }
                    }
                    for c in &mine {
                        self.machine_row(ui, ctx, c);
                    }
                    ui.add_space(18.0);
                }
                if distros.is_empty() {
                    return;
                }
                section(ui, p, "NEW MACHINE");
                let gap = 12.0;
                let width = ui.available_width();
                let columns = (((width + gap) / (CARD_WIDTH + gap)).floor() as usize).max(1);
                let card_width = (width - gap * (columns - 1) as f32) / columns as f32;
                for row in distros.chunks(columns) {
                    ui.horizontal_top(|ui| {
                        ui.spacing_mut().item_spacing.x = gap;
                        for d in row {
                            if card(ui, p, d, card_width) {
                                self.form = Some(MachineForm::new(d, store));
                            }
                        }
                    });
                    ui.add_space(gap);
                }
                ui.label(
                    RichText::new(
                        "Machines share this computer's Linux kernel (or the engine's), so \
                         they start in seconds. They run no desktop and, by default, no \
                         systemd.",
                    )
                    .size(12.0)
                    .color(p.muted),
                );
            });
    }

    /// A machine whose container doesn't exist yet: its download.
    fn starting_row(&self, ui: &mut egui::Ui, ctx: &PageCtx, name: &str) {
        let p = ctx.palette;
        row_frame(ui, p, |ui| {
            ui.spinner();
            ui.label(RichText::new(name).strong().color(p.text));
            let pull = ctx
                .store
                .pulls
                .iter()
                .find(|(_, progress)| !progress.pushing);
            let text = match pull {
                Some((image, progress)) if progress.total > 0 => format!(
                    "Downloading {image}… {:.0}%",
                    progress.done as f32 * 100.0 / progress.total as f32
                ),
                Some((image, _)) => format!("Downloading {image}…"),
                None => "Starting…".to_owned(),
            };
            ui.label(RichText::new(text).size(12.5).color(p.muted));
        });
    }

    fn machine_row(&mut self, ui: &mut egui::Ui, ctx: &mut PageCtx, c: &ContainerInfo) {
        let p = ctx.palette;
        let (distro, tag) = kind(c);
        let busy = creating(ctx.store, &c.name);
        let pending = ctx.store.pending.contains_key(&c.id);
        row_frame(ui, p, |ui| {
            if let Some(d) = distro {
                badge(ui, d, 30.0);
            }
            ui.vertical(|ui| {
                // Room for the state and buttons on the right.
                ui.set_max_width((ui.available_width() - BUTTONS_WIDTH).max(120.0));
                ui.label(RichText::new(&c.name).strong().color(p.text));
                let mut about = distro.map_or(c.image.clone(), |d| format!("{} {tag}", d.name));
                if let Some((_, folder)) = &c.machine
                    && !folder.is_empty()
                {
                    about.push_str(&format!(" · shares {}", tilde(folder)));
                }
                ui.add(egui::Label::new(RichText::new(about).size(12.0).color(p.muted)).truncate());
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if busy {
                    ui.label(
                        RichText::new("Setting up: your user, sudo, git…")
                            .size(12.5)
                            .color(p.muted),
                    );
                    ui.spinner();
                    return;
                }
                if widgets::icon_button(ui, Icon::Close, "Remove the machine").clicked() {
                    ctx.request(confirm_remove(c));
                }
                ui.add_enabled_ui(!pending, |ui| {
                    if c.is_running() {
                        if widgets::icon_button(ui, Icon::Stop, "Stop the machine").clicked() {
                            ctx.request(Intent::Run(task(Action::Stop, c)));
                        }
                        if widgets::primary_button(ui, "Open shell").clicked() {
                            self.open = Some(c.id.clone());
                        }
                    } else if ui.button("Start").clicked() {
                        ctx.request(Intent::Run(task(Action::Start, c)));
                    }
                });
                if c.is_running() {
                    widgets::pill(ui, "Running", p.success);
                } else {
                    widgets::pill(ui, "Stopped", p.muted);
                }
            });
        });
    }

    /// The open machine: a header, then its shell.
    fn machine_view(&mut self, ui: &mut egui::Ui, ctx: &mut PageCtx, c: &ContainerInfo) {
        let p = ctx.palette;
        let (distro, tag) = kind(c);
        ui.horizontal(|ui| {
            if ui.button("← Machines").clicked() {
                self.open = None;
            }
            ui.add_space(6.0);
            if let Some(d) = distro {
                badge(ui, d, 24.0);
            }
            ui.label(
                RichText::new(&c.name)
                    .font(egui::FontId::new(15.0, crate::theme::semibold()))
                    .color(p.text),
            );
            if let Some(d) = distro {
                ui.label(RichText::new(format!("{} {tag}", d.name)).color(p.muted));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let pending = ctx.store.pending.contains_key(&c.id);
                ui.add_enabled_ui(!pending, |ui| {
                    if c.is_running() {
                        if ui.button("Stop").clicked() {
                            ctx.request(Intent::Run(task(Action::Stop, c)));
                        }
                    } else if widgets::primary_button(ui, "Start").clicked() {
                        ctx.request(Intent::Run(task(Action::Start, c)));
                    }
                });
            });
        });
        if let Some((_, folder)) = &c.machine
            && !folder.is_empty()
        {
            ui.label(
                RichText::new(format!("Shares {} from this computer", tilde(folder)))
                    .size(12.0)
                    .color(p.muted),
            );
        }
        ui.add_space(10.0);
        if creating(ctx.store, &c.name) {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(RichText::new("Setting up the machine…").color(p.muted));
            });
            return;
        }
        terminal::show(ui, ctx, &mut self.terminal, c);
    }
}

/// A row of the machine list.
fn row_frame(ui: &mut egui::Ui, p: &Palette, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(p.bg)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(egui::Margin::symmetric(12, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(add);
        });
    ui.add_space(8.0);
}

/// A machine that couldn't be created, and why. True when dismissed.
fn failed_row(ui: &mut egui::Ui, p: &Palette, name: &str, message: &str) -> bool {
    let mut dismissed = false;
    row_frame(ui, p, |ui| {
        ui.label(RichText::new(name).strong().color(p.text));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            dismissed = widgets::icon_button(ui, Icon::Close, "Dismiss").clicked();
            ui.add(egui::Label::new(RichText::new(message).size(12.5).color(p.danger)).truncate())
                .on_hover_text(message);
        });
    });
    dismissed
}

/// A distribution's card. True when Create… is clicked.
fn card(ui: &mut egui::Ui, p: &Palette, d: &'static Distro, width: f32) -> bool {
    let mut clicked = false;
    let layout = egui::Layout::top_down(egui::Align::Min);
    ui.allocate_ui_with_layout(egui::vec2(width, 0.0), layout, |ui| {
        egui::Frame::new()
            .fill(p.bg)
            .stroke(Stroke::new(1.0, p.border))
            .corner_radius(CornerRadius::same(12))
            .inner_margin(egui::Margin::same(14))
            .show(ui, |ui| {
                ui.set_width(width - 30.0);
                ui.horizontal(|ui| {
                    badge(ui, d, 36.0);
                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new(d.name)
                                .font(egui::FontId::new(15.0, crate::theme::semibold()))
                                .color(p.text),
                        );
                        ui.add(
                            egui::Label::new(RichText::new(d.summary).size(12.5).color(p.muted))
                                .truncate(),
                        );
                    });
                });
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    // The button first; the image gets what's left, so a
                    // long name is cut short instead of running under it.
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        clicked = widgets::primary_button(ui, "Create…")
                            .on_hover_text(format!("Create a {} machine", d.name))
                            .clicked();
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            let image = format!("{}:{}", d.image, d.tags[0]);
                            ui.add(
                                egui::Label::new(
                                    RichText::new(&image).monospace().size(11.5).color(p.muted),
                                )
                                .truncate(),
                            )
                            .on_hover_text(image);
                        });
                    });
                });
            });
    });
    clicked
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortens_home_folders() {
        let Ok(home) = std::env::var("HOME") else {
            return;
        };
        assert_eq!(tilde(&format!("{home}/code/app")), "~/code/app");
        assert_eq!(tilde(&home), "~");
        assert_eq!(tilde(&format!("{home}x/y")), format!("{home}x/y"));
        assert_eq!(tilde("/srv/app"), "/srv/app");
    }
}
