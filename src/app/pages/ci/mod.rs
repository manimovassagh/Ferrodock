//! Local CI page: a project's GitHub Actions workflows, run here in
//! containers before pushing, and how those runs went.
//!
//! - `run_form` is the Run dialog (event, inputs, secrets).
//! - `run_view` shows one run: jobs, steps and logs.

mod run_form;
mod run_view;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use eframe::egui::{self, RichText};

use crate::app::components::icons::{self, Icon};
use crate::app::components::widgets;
use crate::app::page::{Command, Intent, Page, PageCtx};
use crate::app::store::{CiRun, Store};
use crate::docker::Resource;
use crate::docker::ci::{CiOp, Workflow, find_workflows};
use crate::theme::Palette;
use run_form::RunForm;
use run_view::{RunView, duration, state_mark};

/// How often the workflow files are read again while the page is open.
const RELOAD_EVERY: Duration = Duration::from_secs(2);

#[derive(Default)]
pub struct CiPage {
    /// The project folder; the most recent one until another is chosen.
    project: Option<PathBuf>,
    /// The folder typed in, where there's no folder picker.
    typed: String,
    /// The project's workflows, or (file, why) one can't be read.
    workflows: Vec<Result<Workflow, (String, String)>>,
    loaded: Option<Instant>,
    form: Option<RunForm>,
    /// The run shown across the page.
    open: Option<u64>,
    view: RunView,
    /// Open the newest run once it shows up (one was just asked for).
    follow: bool,
    /// The run to ask for once the page is drawn with a project, from the
    /// command palette.
    run_asked: Option<String>,
}

impl CiPage {
    fn set_project(&mut self, project: PathBuf) {
        self.project = Some(project);
        self.loaded = None;
        self.workflows.clear();
    }

    /// Reads the workflows again now and then, so edits show up.
    fn reload(&mut self) {
        let Some(project) = &self.project else { return };
        if self.loaded.is_some_and(|at| at.elapsed() < RELOAD_EVERY) {
            return;
        }
        self.workflows = find_workflows(project);
        self.loaded = Some(Instant::now());
    }

    fn open_run(&mut self, id: u64) {
        self.open = Some(id);
        self.view = RunView::default();
    }

    fn ask_to_run(&mut self, workflow: &Workflow) {
        if let Some(project) = &self.project {
            self.form = Some(RunForm::new(project, workflow));
        }
    }
}

fn pick_folder() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Choose a project with GitHub Actions workflows")
        .pick_folder()
}

fn folder_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
}

fn section(ui: &mut egui::Ui, p: &Palette, title: &str) {
    ui.label(
        RichText::new(title)
            .font(egui::FontId::new(11.5, crate::theme::semibold()))
            .color(p.muted),
    );
    ui.add_space(6.0);
}

fn row_frame<R>(ui: &mut egui::Ui, p: &Palette, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let inner = egui::Frame::new()
        .fill(p.bg)
        .stroke(egui::Stroke::new(1.0, p.border))
        .corner_radius(egui::CornerRadius::same(10))
        .inner_margin(egui::Margin::symmetric(12, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(add).inner
        })
        .inner;
    ui.add_space(8.0);
    inner
}

/// The icon in a tinted tile, as cards elsewhere have.
fn tile(ui: &mut egui::Ui, p: &Palette, icon: Icon) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(30.0, 30.0), egui::Sense::hover());
    ui.painter().rect_filled(
        rect,
        egui::CornerRadius::same(8),
        p.accent.gamma_multiply(0.12),
    );
    icons::paint(ui.painter(), rect.shrink(6.0), icon, p.accent);
}

impl Page for CiPage {
    fn title(&self) -> &'static str {
        "Local CI"
    }

    fn icon(&self) -> Icon {
        Icon::Workflow
    }

    fn resource(&self) -> Resource {
        Resource::Containers
    }

    fn badge(&self, store: &Store) -> String {
        match store.ci.running() {
            0 => String::new(),
            n => n.to_string(),
        }
    }

    fn summary(&self, store: &Store) -> String {
        match store.ci.running() {
            0 => "Run your GitHub Actions workflows here, before you push".into(),
            n => format!("{} running", crate::docker::plural(n, "workflow")),
        }
    }

    fn search_hint(&self) -> &'static str {
        "Filter workflows and runs"
    }

    fn bottom(&self) -> bool {
        true
    }

    fn has_overlay(&self) -> bool {
        self.form.is_some()
    }

    fn toolbar(&mut self, ui: &mut egui::Ui, _ctx: &mut PageCtx) {
        if self.open.is_none()
            && self.project.is_some()
            && widgets::button(ui, Icon::Folder, "Change project…").clicked()
            && let Some(folder) = pick_folder()
        {
            self.set_project(folder);
        }
    }

    fn show(&mut self, ui: &mut egui::Ui, ctx: &mut PageCtx) {
        let p = ctx.palette;
        if self.project.is_none()
            && let Some(recent) = ctx.store.recent_ci.first()
            && Path::new(recent).is_dir()
        {
            self.set_project(PathBuf::from(recent));
        }
        self.reload();
        if let Some(file) = self.run_asked.take()
            && let Some(Ok(workflow)) = self
                .workflows
                .iter()
                .find(|w| w.as_ref().is_ok_and(|w| w.file == file))
                .cloned()
        {
            self.ask_to_run(&workflow);
        }
        if let Some(form) = &mut self.form {
            match run_form::show(ui, p, form) {
                Some(run_form::Outcome::Run(request)) => {
                    if let Err(e) = form.save_secrets() {
                        ctx.request(Intent::Notify {
                            text: e,
                            is_error: true,
                        });
                    }
                    ctx.request(Intent::Ci(CiOp::Run(request)));
                    self.follow = true;
                    self.form = None;
                }
                Some(run_form::Outcome::Cancel) => self.form = None,
                None => {}
            }
        }
        if self.follow
            && let Some(newest) = ctx.store.ci.runs.first()
            && Some(newest.id) != self.open
            && newest.started.elapsed() < Duration::from_secs(2)
        {
            self.follow = false;
            self.open_run(newest.id);
        }
        let open = self.open.and_then(|id| ctx.store.ci.get(id));
        match open {
            Some(run) => match self.view.show(ui, ctx, run) {
                Some(run_view::Action::Back) => self.open = None,
                Some(run_view::Action::Rerun) => {
                    let mut again = run.request.clone();
                    again.workflow = self
                        .workflows
                        .iter()
                        .flatten()
                        .find(|w| w.file == again.workflow.file)
                        .cloned()
                        .unwrap_or(again.workflow);
                    ctx.request(Intent::Ci(CiOp::Run(Box::new(again))));
                    self.follow = true;
                }
                None => {}
            },
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

    /// Opens a run (by its ID), or the Run dialog of a workflow
    /// (`run:<file>`).
    fn select(&mut self, id: &str) {
        if let Some(file) = id.strip_prefix("run:") {
            self.open = None;
            self.run_asked = Some(file.to_owned());
        } else if let Ok(run) = id.parse() {
            self.open_run(run);
        }
    }

    fn commands(&self, store: &Store) -> Vec<Command> {
        let mut list: Vec<Command> = self
            .workflows
            .iter()
            .flatten()
            .map(|w| {
                Command::select(
                    format!("Run {} locally…", w.name),
                    "Local CI",
                    format!("run:{}", w.file),
                )
            })
            .collect();
        list.extend(store.ci.runs.iter().map(|r| {
            Command::select(
                format!("{} #{}", r.title(), r.number),
                format!("Local CI run · {}", r.state.label()),
                r.id.to_string(),
            )
        }));
        list
    }
}

impl CiPage {
    fn list(&mut self, ui: &mut egui::Ui, ctx: &mut PageCtx) {
        let p = ctx.palette;
        let Some(project) = self.project.clone() else {
            self.choose(ui, ctx);
            return;
        };
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                row_frame(ui, p, |ui| {
                    tile(ui, p, Icon::Folder);
                    ui.vertical(|ui| {
                        ui.label(RichText::new(folder_name(&project)).strong().color(p.text));
                        ui.add(
                            egui::Label::new(
                                RichText::new(project.display().to_string())
                                    .size(12.0)
                                    .color(p.muted),
                            )
                            .truncate(),
                        );
                    });
                    let others: Vec<&String> = ctx
                        .store
                        .recent_ci
                        .iter()
                        .filter(|r| Path::new(r.as_str()) != project)
                        .collect();
                    if !others.is_empty() {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.menu_button("Recent", |ui| {
                                for recent in others {
                                    if ui.button(recent.as_str()).clicked() {
                                        self.set_project(PathBuf::from(recent));
                                    }
                                }
                            });
                        });
                    }
                });
                ui.add_space(6.0);
                self.workflow_list(ui, ctx);
                ui.add_space(10.0);
                self.run_list(ui, ctx, &project);
            });
    }

    /// Asks for a project when there's none yet.
    fn choose(&mut self, ui: &mut egui::Ui, ctx: &mut PageCtx) {
        let p = ctx.palette;
        if widgets::empty_state(
            ui,
            p,
            Icon::Workflow,
            "Run your workflows before you push",
            "Choose a project with .github/workflows. Its jobs run here in containers, \
             step by step as on GitHub, with live logs.",
            Some("Choose project…"),
        ) && let Some(folder) = pick_folder()
        {
            self.set_project(folder);
        }
        ui.vertical_centered(|ui| {
            ui.set_max_width(460.0);
            ui.horizontal(|ui| {
                let field = ui.add(
                    egui::TextEdit::singleline(&mut self.typed)
                        .hint_text("or type the project's folder")
                        .desired_width(360.0),
                );
                let entered = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if (ui.button("Open").clicked() || entered) && !self.typed.trim().is_empty() {
                    let folder = PathBuf::from(self.typed.trim());
                    if folder.is_dir() {
                        self.set_project(folder);
                    } else {
                        ctx.request(Intent::Notify {
                            text: format!("There's no folder {}", folder.display()),
                            is_error: true,
                        });
                    }
                }
            });
        });
    }

    fn workflow_list(&mut self, ui: &mut egui::Ui, ctx: &mut PageCtx) {
        let p = ctx.palette;
        section(ui, p, "WORKFLOWS");
        if self.workflows.is_empty() {
            ui.label(
                RichText::new("There are no workflows in .github/workflows of this project.")
                    .color(p.muted),
            );
            return;
        }
        let mut asked = None;
        for workflow in &self.workflows {
            match workflow {
                Ok(w) => {
                    if !ctx.matches_search(&[&w.name, &w.file]) {
                        continue;
                    }
                    let run = row_frame(ui, p, |ui| {
                        tile(ui, p, Icon::Workflow);
                        ui.vertical(|ui| {
                            ui.set_max_width((ui.available_width() - 120.0).max(140.0));
                            ui.label(RichText::new(&w.name).strong().color(p.text));
                            let mut about = w.file.clone();
                            if !w.on.is_empty() {
                                about.push_str(&format!(" · on {}", w.on.join(", ")));
                            }
                            about.push_str(&format!(
                                " · {}",
                                crate::docker::plural(w.jobs.len(), "job")
                            ));
                            ui.add(
                                egui::Label::new(RichText::new(about).size(12.0).color(p.muted))
                                    .truncate(),
                            );
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            widgets::primary_button(ui, "Run…").clicked()
                        })
                        .inner
                    });
                    if run {
                        asked = Some(w.clone());
                    }
                }
                Err((file, why)) => {
                    if !ctx.matches_search(&[file]) {
                        continue;
                    }
                    row_frame(ui, p, |ui| {
                        tile(ui, p, Icon::File);
                        ui.vertical(|ui| {
                            ui.label(RichText::new(file).strong().color(p.text));
                            ui.add(
                                egui::Label::new(RichText::new(why).size(12.0).color(p.danger))
                                    .wrap(),
                            );
                        });
                    });
                }
            }
        }
        if let Some(workflow) = asked {
            self.ask_to_run(&workflow);
        }
    }

    fn run_list(&mut self, ui: &mut egui::Ui, ctx: &mut PageCtx, project: &Path) {
        let p = ctx.palette;
        let runs: Vec<&CiRun> = ctx
            .store
            .ci
            .runs
            .iter()
            .filter(|r| r.request.project == project && ctx.matches_search(&[r.title()]))
            .collect();
        if runs.is_empty() {
            return;
        }
        section(ui, p, "RUNS");
        let mut opened = None;
        let mut cancel = None;
        for run in runs {
            let clicked = row_frame(ui, p, |ui| {
                state_mark(ui, p, run.state, 18.0);
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new(format!("{} #{}", run.title(), run.number))
                            .strong()
                            .color(p.text),
                    );
                    ui.label(
                        RichText::new(format!(
                            "{} · {} · {}",
                            run.request.event,
                            run.state.label(),
                            duration(run.duration())
                        ))
                        .size(12.0)
                        .color(p.muted),
                    );
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let open = ui.button("Open").clicked();
                    if !run.state.done()
                        && widgets::icon_button(ui, Icon::Stop, "Cancel the run").clicked()
                    {
                        cancel = Some(run.id);
                    }
                    open
                })
                .inner
            });
            if clicked {
                opened = Some(run.id);
            }
        }
        if let Some(id) = cancel {
            ctx.request(Intent::Ci(CiOp::Cancel(id)));
        }
        if let Some(id) = opened {
            self.open_run(id);
        }
    }
}
