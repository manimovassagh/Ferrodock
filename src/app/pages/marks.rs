//! Ticking several images, volumes or networks to remove them together:
//! the tick boxes, Select all, and the bar with Remove.

use std::collections::BTreeSet;

use eframe::egui;

use super::remove_task;
use crate::app::components::icons::Icon;
use crate::app::components::{table, widgets};
use crate::app::page::{Confirmation, Intent, PageCtx};
use crate::docker::{Resource, Task, plural};
use crate::theme::Palette;

/// How many names the removal question lists before "and n more".
const LISTED: usize = 6;

/// One row that can be ticked.
pub struct Item<'a> {
    /// What the Engine expects (a volume's name, everything else's ID).
    pub id: &'a str,
    pub name: &'a str,
    /// Whether something still uses it, so removing it would fail.
    pub in_use: bool,
}

/// The ticked rows of one page.
#[derive(Default)]
pub struct Marks {
    ids: BTreeSet<String>,
}

impl Marks {
    pub fn contains(&self, id: &str) -> bool {
        self.ids.contains(id)
    }

    pub fn clear(&mut self) {
        self.ids.clear();
    }

    pub fn select_all(&mut self, items: &[Item]) {
        self.ids = items.iter().map(|i| i.id.to_owned()).collect();
    }

    /// The tick-box column, with Select all in its header.
    pub fn column(&self, items: &[Item]) -> table::Col {
        let ticked = items.iter().filter(|i| self.contains(i.id)).count();
        table::Col::ticks(table::Ticked::of(ticked, items.len()))
    }

    /// The tick box of one row. Returns the new state when it's clicked,
    /// to hand to [`Marks::apply`] once the table is drawn.
    pub fn cell(&self, ui: &mut egui::Ui, p: &Palette, item: &Item) -> Option<(String, bool)> {
        let on = self.contains(item.id);
        table::tick(ui, p, on, item.name).then(|| (item.id.to_owned(), !on))
    }

    /// Applies the row clicks and Select all of table `table_id`, and
    /// forgets rows that are gone.
    pub fn apply(
        &mut self,
        ui: &egui::Ui,
        table_id: &str,
        items: &[Item],
        clicks: Vec<(String, bool)>,
    ) {
        let all_clicked = table::select_all_clicked(ui, table_id);
        self.update(all_clicked, items, clicks);
    }

    fn update(&mut self, all_clicked: bool, items: &[Item], clicks: Vec<(String, bool)>) {
        if all_clicked {
            if items.iter().all(|i| self.contains(i.id)) {
                self.ids.clear();
            } else {
                self.select_all(items);
            }
        }
        for (id, on) in clicks {
            if on {
                self.ids.insert(id);
            } else {
                self.ids.remove(&id);
            }
        }
        self.ids.retain(|id| items.iter().any(|i| i.id == id));
    }

    /// "3 selected · 1 in use" with Remove and Clear, while any is ticked.
    /// `kind` is the singular noun, e.g. "volume".
    pub fn bar(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &mut PageCtx,
        kind: &str,
        resource: Resource,
        items: &[Item],
    ) {
        if self.ids.is_empty() {
            return;
        }
        let p = ctx.palette;
        let chosen: Vec<&Item> = items.iter().filter(|i| self.contains(i.id)).collect();
        let in_use = chosen.iter().filter(|i| i.in_use).count();
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
                    if in_use > 0 {
                        ui.label(
                            egui::RichText::new(format!("· {in_use} in use"))
                                .size(13.0)
                                .color(p.muted),
                        );
                    }
                    ui.add_space(8.0);
                    let free = chosen.len() - in_use;
                    let remove = ui
                        .add_enabled_ui(free > 0, |ui| {
                            widgets::danger_button(ui, p, &format!("Remove {free}…"))
                        })
                        .inner
                        .on_disabled_hover_text("All of them are in use");
                    if remove.clicked() {
                        ctx.request(confirm_removal(kind, resource, &chosen));
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if widgets::icon_button(ui, Icon::Close, "Clear the selection").clicked() {
                            self.ids.clear();
                        }
                    });
                });
            });
        ui.add_space(6.0);
    }
}

/// "Remove 3 volumes?", listing them; ones in use are left out and said so.
fn confirm_removal(kind: &str, resource: Resource, chosen: &[&Item]) -> Intent {
    let (free, in_use): (Vec<&&Item>, Vec<&&Item>) = chosen.iter().partition(|i| !i.in_use);
    let mut names: Vec<String> = free
        .iter()
        .take(LISTED)
        .map(|i| i.name.to_owned())
        .collect();
    if free.len() > LISTED {
        names.push(format!("and {} more", free.len() - LISTED));
    }
    let mut message = format!("{}.\n\nThis can't be undone.", names.join(", "));
    if !in_use.is_empty() {
        message.push_str(&format!(
            " {} in use, so kept.",
            if in_use.len() == 1 {
                format!("1 {kind} is")
            } else {
                format!("{} are", plural(in_use.len(), kind))
            }
        ));
    }
    Intent::Confirm(Confirmation {
        title: format!("Remove {}?", plural(free.len(), kind)),
        message,
        action_label: format!("Remove {}", free.len()),
        tasks: free
            .iter()
            .map(|i| remove_task(resource, i.id, i.name))
            .collect::<Vec<Task>>(),
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item<'a>(name: &'a str, in_use: bool) -> Item<'a> {
        Item {
            id: name,
            name,
            in_use,
        }
    }

    #[test]
    fn removal_keeps_what_is_in_use() {
        let a = item("cache", false);
        let b = item("db", true);
        let Intent::Confirm(c) = confirm_removal("volume", Resource::Volumes, &[&a, &b]) else {
            panic!("expected a confirmation");
        };
        assert_eq!(c.title, "Remove 1 volume?");
        assert!(c.message.contains("1 volume is in use, so kept."));
        assert_eq!(c.tasks.len(), 1);
        assert_eq!(c.tasks[0].id, "cache");
    }

    #[test]
    fn forgets_rows_that_are_gone() {
        let mut marks = Marks::default();
        marks.select_all(&[item("a", false), item("b", false)]);
        marks.update(false, &[item("a", false)], vec![]);
        assert!(marks.contains("a") && !marks.contains("b"));
        marks.update(true, &[item("a", false)], vec![]);
        assert!(!marks.contains("a"));
        marks.update(false, &[item("a", false)], vec![("a".into(), true)]);
        assert!(marks.contains("a"));
    }
}
