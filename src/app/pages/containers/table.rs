//! The container list.

use eframe::egui;

use super::{ContainersPage, actions, ports, status};
use crate::app::components::icons::Icon;
use std::collections::VecDeque;

use crate::app::components::{sparkline, table, widgets};
use crate::app::page::{PageCtx, toggle_selection};
use crate::app::pages::{confirm_removal, remove_task};
use crate::docker::{Action, ContainerInfo, Resource, StatsSample, Task, format_size};
use crate::theme::Palette;

pub(super) fn show(
    page: &mut ContainersPage,
    ui: &mut egui::Ui,
    ctx: &mut PageCtx,
    visible: &[&ContainerInfo],
) {
    let p = ctx.palette;
    let store = ctx.store;

    // Drop the less important columns when space is tight (e.g. with the
    // details pane open); both are still shown in the details pane.
    let width = ui.available_width();
    let show_ports = width >= 700.0;
    let show_id = width >= 560.0;
    let show_usage = width >= 860.0;
    let mut columns = vec![table::Col::fill("NAME"), table::Col::fixed("STATUS", 150.0)];
    if show_id {
        columns.push(table::Col::fixed("ID", 110.0));
    }
    if show_usage {
        columns.push(table::Col::fixed("CPU", 104.0));
        columns.push(table::Col::fixed("MEMORY", 90.0));
    }
    if show_ports {
        columns.push(table::Col::fixed("PORTS", 140.0));
    }
    columns.push(table::Col::fixed("", 100.0));

    if !page.marked.is_empty() {
        bulk_bar(page, ui, ctx, visible);
    }

    let selected = page.selected.clone();
    let marked = page.marked.clone();
    let clicked = table::show(
        ui,
        p,
        "containers",
        columns,
        visible.len(),
        |i| selected.as_deref() == Some(visible[i].id.as_str()) || marked.contains(&visible[i].id),
        |i, row| {
            let c = visible[i];
            row.col(|ui| {
                let color = status::color(p, &c.state);
                table::with_icon(ui, p, Icon::Container, color, &c.name, &c.image)
            });
            row.col(|ui| status::cell(ui, p, c, store.pending.get(&c.id).copied()));
            if show_id {
                row.col(|ui| table::mono(ui, p, c.short_id()));
            }
            if show_usage {
                let history = store.usage.get(&c.id);
                row.col(|ui| cpu_cell(ui, p, history));
                row.col(|ui| {
                    let memory = history
                        .and_then(|h| h.back())
                        .map(|s| format_size(s.memory_used as i64));
                    table::muted(ui, p, memory.as_deref().unwrap_or("—"));
                });
            }
            if show_ports {
                row.col(|ui| ports::cell(ui, p, &c.ports));
            }
            let selected_row = selected.as_deref() == Some(c.id.as_str());
            row.col(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                let busy = store.pending.contains_key(&c.id);
                // Actions show on the hovered or selected row only, so idle
                // rows stay calm.
                if (busy || selected_row || table::row_hovered(ui))
                    && let Some(action) = actions::buttons(ui, c, busy, false)
                {
                    ctx.request(actions::intent(action, c));
                }
            });
        },
    );
    if let Some(i) = clicked {
        let modifiers = ui.input(|input| input.modifiers);
        let id = visible[i].id.clone();
        if modifiers.shift && !page.marked.is_empty() {
            // Mark everything between the last marked row and this one.
            let anchor = page
                .anchor
                .as_ref()
                .and_then(|a| visible.iter().position(|c| c.id == *a))
                .unwrap_or(i);
            let (from, to) = (anchor.min(i), anchor.max(i));
            page.marked
                .extend(visible[from..=to].iter().map(|c| c.id.clone()));
        } else if modifiers.command {
            if !page.marked.remove(&id) {
                // Start the selection with the row that was open, if any.
                if let Some(open) = page.selected.take() {
                    page.marked.insert(open);
                }
                page.marked.insert(id.clone());
            }
            page.anchor = Some(id);
        } else {
            page.marked.clear();
            toggle_selection(&mut page.selected, id.clone());
            page.anchor = Some(id);
        }
    }
}

/// CPU now, with a small trend line; amber above 80%.
fn cpu_cell(ui: &mut egui::Ui, p: &Palette, history: Option<&VecDeque<StatsSample>>) {
    let Some(latest) = history.and_then(|h| h.back()) else {
        table::muted(ui, p, "—");
        return;
    };
    let busy = latest.cpu_percent >= 80.0;
    let color = if busy { p.warning } else { p.accent };
    ui.add(
        egui::Label::new(
            egui::RichText::new(format!("{:.1}%", latest.cpu_percent)).color(if busy {
                p.warning
            } else {
                p.text
            }),
        )
        .selectable(false),
    );
    let values: Vec<f64> = history
        .into_iter()
        .flatten()
        .map(|s| s.cpu_percent)
        .collect();
    let max = values.iter().copied().fold(10.0, f64::max);
    sparkline::tiny(ui, &values, max, color, egui::vec2(36.0, 14.0));
}

/// "3 selected" with the actions that apply to them.
fn bulk_bar(
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
                let stopped: Vec<Task> = chosen
                    .iter()
                    .filter(|c| !c.is_active())
                    .map(|c| remove_task(Resource::Containers, &c.id, &c.name))
                    .collect();
                let remove = ui
                    .add_enabled_ui(!stopped.is_empty(), |ui| {
                        widgets::danger_button(ui, p, "Remove…")
                    })
                    .inner
                    .on_disabled_hover_text("Stop them first");
                if remove.clicked() {
                    ctx.request(confirm_removal("container", stopped));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Clear").clicked() {
                        page.marked.clear();
                    }
                });
            });
        });
    ui.add_space(6.0);
}
