//! The container list, optionally grouped by Compose project under
//! collapsible headers.

use eframe::egui;

use super::{ContainersPage, actions, ports, status};
use crate::app::components::icons::Icon;
use std::collections::VecDeque;

use crate::app::components::{sparkline, table, widgets};
use crate::app::page::{PageCtx, toggle_selection};
use crate::docker::{Action, ContainerInfo, StatsSample, Task, format_size};
use crate::theme::Palette;

/// One line of the list.
enum Row<'a> {
    /// A Compose project (None: containers in no project) and its
    /// containers.
    Group {
        project: Option<&'a str>,
        containers: Vec<&'a ContainerInfo>,
    },
    /// A container; `grouped` indents it under its project.
    Item {
        container: &'a ContainerInfo,
        grouped: bool,
    },
}

fn grouping_id() -> egui::Id {
    egui::Id::unique("containers-grouped-by-project")
}

/// Whether the list is grouped by Compose project (remembered between runs).
fn grouped(ui: &egui::Ui) -> bool {
    ui.data_mut(|d| d.get_persisted::<bool>(grouping_id()))
        .unwrap_or(false)
}

/// The "Group by project" switch.
pub(super) fn grouping_toggle(ui: &mut egui::Ui) {
    let mut on = grouped(ui);
    if ui
        .toggle_value(&mut on, "Group by project")
        .on_hover_text("Show containers under their Compose project")
        .changed()
    {
        ui.data_mut(|d| d.insert_persisted(grouping_id(), on));
    }
}

/// The rows to show: containers as they are, or grouped by project with
/// folded projects' containers left out.
fn rows<'a>(
    visible: &[&'a ContainerInfo],
    grouped: bool,
    collapsed: &std::collections::BTreeSet<String>,
) -> Vec<Row<'a>> {
    if !grouped {
        return visible
            .iter()
            .map(|c| Row::Item {
                container: c,
                grouped: false,
            })
            .collect();
    }
    let mut projects: std::collections::BTreeMap<&str, Vec<&ContainerInfo>> = Default::default();
    let mut loose = Vec::new();
    for c in visible {
        match &c.compose {
            Some(labels) => projects.entry(labels.project.as_str()).or_default().push(c),
            None => loose.push(*c),
        }
    }
    let mut groups: Vec<(Option<&str>, Vec<&ContainerInfo>)> =
        projects.into_iter().map(|(p, cs)| (Some(p), cs)).collect();
    if !loose.is_empty() {
        groups.push((None, loose));
    }
    let mut rows = Vec::new();
    for (project, containers) in groups {
        let folded = collapsed.contains(project.unwrap_or(""));
        let items: Vec<Row> = if folded {
            Vec::new()
        } else {
            containers
                .iter()
                .map(|c| Row::Item {
                    container: c,
                    grouped: true,
                })
                .collect()
        };
        rows.push(Row::Group {
            project,
            containers,
        });
        rows.extend(items);
    }
    rows
}

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
    let ticked = visible
        .iter()
        .filter(|c| page.marked.contains(&c.id))
        .count();
    let mut columns = vec![
        table::Col::ticks(table::Ticked::of(ticked, visible.len())),
        table::Col::fill("NAME"),
        table::Col::fixed("STATUS", 150.0),
    ];
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
        super::bulk::bar(page, ui, ctx, visible);
    }

    let selected = page.selected.clone();
    let marked = page.marked.clone();
    let rows = rows(visible, grouped(ui), &page.collapsed);
    let columns_count = columns.len();
    let mut ticks: Vec<(Vec<String>, bool)> = Vec::new();
    let clicked = table::show(
        ui,
        p,
        "containers",
        columns,
        rows.len(),
        |i| match &rows[i] {
            Row::Item { container: c, .. } => {
                selected.as_deref() == Some(c.id.as_str()) || marked.contains(&c.id)
            }
            Row::Group { .. } => false,
        },
        |i, row| {
            let (c, indented) = match &rows[i] {
                Row::Item { container, grouped } => (*container, *grouped),
                Row::Group {
                    project,
                    containers,
                } => {
                    let folded = page.collapsed.contains(project.unwrap_or(""));
                    let all = containers.iter().all(|c| marked.contains(&c.id));
                    let state = table::Ticked::of(
                        containers.iter().filter(|c| marked.contains(&c.id)).count(),
                        containers.len(),
                    );
                    row.col(|ui| {
                        let name = project.unwrap_or("other containers");
                        if group_tick(ui, p, state, name) {
                            let ids = containers.iter().map(|c| c.id.clone()).collect();
                            ticks.push((ids, !all));
                        }
                    });
                    group_row(row, ctx, *project, containers, folded, columns_count - 1);
                    return;
                }
            };
            row.col(|ui| {
                let on = marked.contains(&c.id);
                if table::tick(ui, p, on, &c.name) {
                    ticks.push((vec![c.id.clone()], !on));
                }
            });
            row.col(|ui| {
                if indented {
                    ui.add_space(18.0);
                }
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
    if table::select_all_clicked(ui, "containers") {
        if ticked >= visible.len() {
            page.marked.clear();
        } else {
            page.marked.extend(visible.iter().map(|c| c.id.clone()));
        }
    }
    for (ids, on) in ticks {
        // The row open in the details pane joins the selection.
        if on
            && page.marked.is_empty()
            && let Some(open) = page.selected.take()
        {
            page.marked.insert(open);
        }
        for id in ids {
            if on {
                page.marked.insert(id);
            } else {
                page.marked.remove(&id);
            }
        }
    }
    let clicked = match clicked.map(|i| &rows[i]) {
        Some(Row::Group { project, .. }) => {
            // Fold or unfold the project.
            let key = project.unwrap_or("").to_owned();
            if !page.collapsed.remove(&key) {
                page.collapsed.insert(key);
            }
            None
        }
        Some(Row::Item { container, .. }) => visible.iter().position(|c| c.id == container.id),
        None => None,
    };
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

/// The tick box of a project's header, which (un)ticks all of it.
fn group_tick(ui: &mut egui::Ui, p: &Palette, state: table::Ticked, name: &str) -> bool {
    table::tick_state(ui, p, state, &format!("Select {name}"))
}

/// A project's header: fold arrow, name, how many run, and actions for
/// all of its containers.
fn group_row(
    row: &mut egui_extras::TableRow<'_, '_>,
    ctx: &mut PageCtx,
    project: Option<&str>,
    containers: &[&ContainerInfo],
    folded: bool,
    columns: usize,
) {
    let p = ctx.palette;
    let running = containers.iter().filter(|c| c.is_active()).count();
    row.col(|ui| {
        let chevron = if folded {
            Icon::ChevronRight
        } else {
            Icon::ChevronDown
        };
        let (rect, _) = ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
        crate::app::components::icons::paint(ui.painter(), rect, chevron, p.muted);
        let name = project.unwrap_or("Other containers");
        ui.label(
            egui::RichText::new(name)
                .font(egui::FontId::new(14.0, crate::theme::semibold()))
                .color(p.text),
        );
        let count = if project.is_some() {
            format!("{running}/{} running", containers.len())
        } else {
            format!("{running} running")
        };
        ui.label(egui::RichText::new(count).size(12.0).color(p.muted));
    });
    // Empty cells up to the actions column.
    for _ in 1..columns - 1 {
        row.col(|_| {});
    }
    row.col(|ui| {
        if project.is_none() {
            return;
        }
        ui.spacing_mut().item_spacing.x = 2.0;
        let tasks = |action: Action, wanted: bool| -> Vec<Task> {
            containers
                .iter()
                .filter(|c| c.is_active() == wanted)
                .map(|c| Task {
                    action,
                    id: c.id.clone(),
                    name: c.name.clone(),
                })
                .collect()
        };
        let mut run = Vec::new();
        if running < containers.len() && widgets::icon_button(ui, Icon::Play, "Start all").clicked()
        {
            run = tasks(Action::Start, false);
        }
        if running > 0 {
            if widgets::icon_button(ui, Icon::Stop, "Stop all").clicked() {
                run = tasks(Action::Stop, true);
            }
            if widgets::icon_button(ui, Icon::Restart, "Restart all").clicked() {
                run = tasks(Action::Restart, true);
            }
        }
        for task in run {
            ctx.request(crate::app::page::Intent::Run(task));
        }
    });
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::docker::ComposeLabels;

    fn container(name: &str, project: Option<&str>) -> ContainerInfo {
        ContainerInfo {
            id: name.into(),
            name: name.into(),
            compose: project.map(|p| ComposeLabels {
                project: p.into(),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    fn shape(rows: &[Row]) -> Vec<String> {
        rows.iter()
            .map(|r| match r {
                Row::Group { project, .. } => format!("# {}", project.unwrap_or("other")),
                Row::Item { container, .. } => container.name.clone(),
            })
            .collect()
    }

    #[test]
    fn groups_and_folds_projects() {
        let all = [
            container("api", Some("shop")),
            container("lone", None),
            container("blog-web", Some("blog")),
            container("db", Some("shop")),
        ];
        let visible: Vec<&ContainerInfo> = all.iter().collect();
        let mut folded = std::collections::BTreeSet::new();
        assert_eq!(shape(&rows(&visible, false, &folded)).len(), 4);
        assert_eq!(
            shape(&rows(&visible, true, &folded)),
            [
                "# blog", "blog-web", "# shop", "api", "db", "# other", "lone"
            ]
        );
        folded.insert("shop".to_owned());
        assert_eq!(
            shape(&rows(&visible, true, &folded)),
            ["# blog", "blog-web", "# shop", "# other", "lone"]
        );
    }
}
