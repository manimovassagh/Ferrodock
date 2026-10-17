//! Compose page: containers grouped by Docker Compose project, with
//! project-wide start / stop / restart / remove; bringing projects up from
//! their files, rebuilding services, and merged logs.

mod logs;
mod up;

use eframe::egui::{self, Color32, RichText};

use crate::app::components::icons::Icon;
use crate::app::components::{details, table, widgets};
use crate::app::page::{
    Command, CommandRun, Confirmation, Intent, Page, PageCtx, toggle_selection,
};
use crate::app::store::Store;
use crate::docker::{
    Action, ComposeAction, ComposeOp, ComposeProject, ComposeTarget, Resource, Task, next_job,
    plural,
};
use crate::theme::Palette;

#[derive(Default, Clone, Copy, PartialEq, Eq)]
enum Tab {
    #[default]
    Overview,
    Logs,
}

#[derive(Default)]
pub struct ComposePage {
    /// Project name.
    selected: Option<String>,
    tab: Tab,
    /// What's typed in the Compose file box.
    up_path: String,
    /// The command whose output was closed.
    closed_output: Option<u64>,
    /// The service the logs are limited to.
    only: Option<String>,
}

/// The project as Compose sees it, if its files are on this computer.
fn target(project: &ComposeProject) -> Option<ComposeTarget> {
    let files: Vec<std::path::PathBuf> = project
        .config_files
        .iter()
        .map(std::path::PathBuf::from)
        .collect();
    (!files.is_empty() && files.iter().all(|f| f.is_file())).then(|| ComposeTarget {
        name: Some(project.name.clone()),
        files,
        dir: project.working_dir.clone().into(),
    })
}

fn run(ctx: &mut PageCtx, target: ComposeTarget, action: ComposeAction) {
    ctx.request(Intent::Compose(ComposeOp::Run {
        job: next_job(),
        target,
        action,
    }));
}

impl ComposePage {
    fn selected<'a>(&self, store: &'a Store) -> Option<&'a ComposeProject> {
        let name = self.selected.as_deref()?;
        store.projects.iter().find(|p| p.name == name)
    }
}

impl Page for ComposePage {
    fn title(&self) -> &'static str {
        "Compose"
    }

    fn icon(&self) -> Icon {
        Icon::Stack
    }

    fn resource(&self) -> Resource {
        Resource::Containers
    }

    fn badge(&self, store: &Store) -> String {
        store.projects.len().to_string()
    }

    fn summary(&self, store: &Store) -> String {
        let running = store.projects.iter().filter(|p| p.running() > 0).count();
        format!(
            "{} · {running} running",
            plural(store.projects.len(), "project")
        )
    }

    fn search_hint(&self) -> &'static str {
        "Search project or service"
    }

    fn show(&mut self, ui: &mut egui::Ui, ctx: &mut PageCtx) {
        let p = ctx.palette;
        let store = ctx.store;
        up::bar(ui, ctx, &mut self.up_path);
        let job = store.compose.run.as_ref().map(|j| j.id);
        if job.is_some() && job != self.closed_output && !up::output(ui, ctx) {
            self.closed_output = job;
        }
        let visible: Vec<&ComposeProject> = store
            .projects
            .iter()
            .filter(|project| ctx.matches_search(&[&project.name, &project.services().join(" ")]))
            .collect();
        if visible.is_empty() {
            super::nothing_to_show(
                ui,
                ctx,
                !store.projects.is_empty(),
                (
                    Icon::Stack,
                    "No Compose projects",
                    "Start one from its compose.yaml above, or with `docker compose up`.",
                ),
                None,
            );
            return;
        }

        let columns = vec![
            table::Col::fill("PROJECT"),
            table::Col::fixed("STATUS", 150.0),
            table::Col::fixed("", 68.0),
        ];
        let selected = self.selected.clone();
        let clicked = table::show(
            ui,
            p,
            "compose",
            columns,
            visible.len(),
            |i| selected.as_deref() == Some(visible[i].name.as_str()),
            |i, row| {
                let project = visible[i];
                let services = plural(project.services().len(), "service");
                row.col(|ui| {
                    let color = if project.running() > 0 {
                        p.success
                    } else {
                        p.muted
                    };
                    table::with_icon(ui, p, Icon::Stack, color, &project.name, &services)
                });
                row.col(|ui| state_pill(ui, p, project));
                row.col(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    for task in project_buttons(ui, store, project, false) {
                        ctx.request(Intent::Run(task));
                    }
                });
            },
        );
        if let Some(i) = clicked {
            toggle_selection(&mut self.selected, visible[i].name.clone());
        }
    }

    fn has_selection(&self, store: &Store) -> bool {
        self.selected(store).is_some()
    }

    fn show_details(&mut self, ui: &mut egui::Ui, ctx: &mut PageCtx) {
        let p = ctx.palette;
        let store = ctx.store;
        let Some(project) = self.selected(store) else {
            return;
        };

        if details::header(ui, p, &project.name) {
            self.selected = None;
        }
        state_pill(ui, p, project);
        ui.add_space(10.0);
        let target = target(project);
        ui.horizontal(|ui| {
            for task in project_buttons(ui, store, project, true) {
                ctx.request(Intent::Run(task));
            }
            if !is_busy(store, project) {
                remove_project_button(ui, ctx, project);
            }
        });
        ui.horizontal(|ui| {
            let missing = "Its Compose files aren't on this computer";
            let rebuild = ui
                .add_enabled(target.is_some(), egui::Button::new("Rebuild all").small())
                .on_hover_text("docker compose up -d --build")
                .on_disabled_hover_text(missing);
            let pull = ui
                .add_enabled(
                    target.is_some(),
                    egui::Button::new("Pull & recreate").small(),
                )
                .on_hover_text("docker compose pull, then up -d --force-recreate")
                .on_disabled_hover_text(missing);
            if let Some(target) = &target {
                if rebuild.clicked() {
                    run(ctx, target.clone(), ComposeAction::Rebuild(None));
                }
                if pull.clicked() {
                    run(ctx, target.clone(), ComposeAction::PullRecreate);
                }
            }
        });
        ui.add_space(12.0);

        widgets::segmented(ui, p, |ui| {
            for (tab, label) in [(Tab::Overview, "Overview"), (Tab::Logs, "Logs")] {
                if widgets::tab(ui, p, label, None, self.tab == tab).clicked() {
                    self.tab = tab;
                }
            }
        });
        ui.add_space(10.0);
        if self.tab == Tab::Logs {
            logs::show(ui, ctx, project, &mut self.only);
            return;
        }
        // Nobody's reading the logs: stop following them.
        if let Some(job) = store.compose.logs.as_ref().filter(|j| j.ended.is_none()) {
            ctx.request(Intent::Compose(ComposeOp::Cancel { job: job.id }));
        }

        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                details::section(ui, p, "Services", |ui| {
                    for c in &project.containers {
                        let service = c.compose.as_ref().map_or("", |l| l.service.as_str());
                        ui.label(RichText::new(service).color(p.muted));
                        ui.horizontal(|ui| {
                            widgets::dot(ui, if c.is_active() { p.success } else { p.muted });
                            ui.label(RichText::new(&c.name).color(p.text));
                            super::containers::ports::links(ui, p, &c.ports);
                            if let Some(target) = &target {
                                let rebuild = ui.small_button("Rebuild").on_hover_text(format!(
                                    "docker compose up -d --build --no-deps {service}"
                                ));
                                if rebuild.clicked() {
                                    run(
                                        ctx,
                                        target.clone(),
                                        ComposeAction::Rebuild(Some(service.to_owned())),
                                    );
                                }
                            }
                        });
                        ui.end_row();
                    }
                });
                details::section(ui, p, "Files", |ui| {
                    details::field(ui, p, "Directory", &project.working_dir);
                    details::field(
                        ui,
                        p,
                        "Config",
                        &widgets::join_or_dash(&project.config_files),
                    );
                });
            });
    }

    fn clear_selection(&mut self) {
        self.selected = None;
    }

    fn select(&mut self, id: &str) {
        self.selected = Some(id.to_owned());
    }

    fn commands(&self, store: &Store) -> Vec<Command> {
        let mut list = Vec::new();
        for project in &store.projects {
            let status = format!(
                "Compose project · {}/{} running",
                project.running(),
                project.containers.len()
            );
            list.push(Command::select(&project.name, status, &project.name));
            let toggle = if project.running() > 0 {
                (Action::Stop, "Stop")
            } else {
                (Action::Start, "Start")
            };
            for (action, verb) in [toggle, (Action::Restart, "Restart")] {
                list.push(Command {
                    label: format!("{verb} project {}", project.name),
                    detail: "Compose project".into(),
                    run: CommandRun::Intents(
                        tasks_for(project, action)
                            .into_iter()
                            .map(Intent::Run)
                            .collect(),
                    ),
                });
            }
        }
        list
    }
}

/// "Running 2/3": green when everything runs, amber when some do.
fn state_pill(ui: &mut egui::Ui, p: &Palette, project: &ComposeProject) {
    let total = project.containers.len();
    let running = project.running();
    let (label, color): (String, Color32) = match running {
        0 => ("Stopped".into(), p.muted),
        n if n == total => ("Running".into(), p.success),
        n => (format!("Running {n}/{total}"), p.warning),
    };
    widgets::pill(ui, &label, color);
}

fn is_busy(store: &Store, project: &ComposeProject) -> bool {
    project
        .containers
        .iter()
        .any(|c| store.pending.contains_key(&c.id))
}

/// Start-all / Stop-all and Restart-all. Returns the tasks to run when one
/// is clicked (empty otherwise).
fn project_buttons(
    ui: &mut egui::Ui,
    store: &Store,
    project: &ComposeProject,
    with_labels: bool,
) -> Vec<Task> {
    if is_busy(store, project) {
        ui.spinner();
        return Vec::new();
    }
    let any_running = project.running() > 0;
    let toggle = if any_running {
        (Action::Stop, Icon::Stop, "Stop all")
    } else {
        (Action::Start, Icon::Play, "Start all")
    };
    let mut clicked = None;
    for (action, icon, label) in [toggle, (Action::Restart, Icon::Restart, "Restart all")] {
        let response = if with_labels {
            widgets::button(ui, icon, label)
        } else {
            widgets::icon_button(ui, icon, label)
        };
        if response.clicked() {
            clicked = Some(action);
        }
    }
    clicked
        .map(|action| tasks_for(project, action))
        .unwrap_or_default()
}

/// One task per container the action applies to: stop only what runs,
/// start only what's stopped, restart everything.
fn tasks_for(project: &ComposeProject, action: Action) -> Vec<Task> {
    project
        .containers
        .iter()
        .filter(|c| match action {
            Action::Start => !c.is_active(),
            Action::Stop => c.is_active(),
            _ => true,
        })
        .map(|c| Task {
            action,
            id: c.id.clone(),
            name: c.name.clone(),
        })
        .collect()
}

fn remove_project_button(ui: &mut egui::Ui, ctx: &mut PageCtx, project: &ComposeProject) {
    if project.running() > 0 {
        ui.add_enabled(false, egui::Button::new("Remove"))
            .on_disabled_hover_text("Stop the project first");
    } else if widgets::danger_button(ui, ctx.palette, "Remove").clicked() {
        let tasks = tasks_for(project, Action::Remove(Resource::Containers));
        ctx.request(Intent::Confirm(Confirmation {
            title: format!("Remove project “{}”?", project.name),
            message: format!(
                "This removes its {}. Volumes, networks and images are kept, and \
                 `docker compose up` recreates everything.",
                plural(tasks.len(), "container")
            ),
            action_label: "Remove".into(),
            tasks,
        }));
    }
}
