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
    /// The workflow whose runs are listed (its file); None lists all.
    selected: Option<String>,
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
        self.selected = None;
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
    /// The project's workflows on the left, and the runs of the chosen
    /// one (or all) on the right, a line each, as GitHub shows them.
    fn list(&mut self, ui: &mut egui::Ui, ctx: &mut PageCtx) {
        let Some(project) = self.project.clone() else {
            self.choose(ui, ctx);
            return;
        };
        self.project_line(ui, ctx, &project);
        ui.add_space(10.0);
        let height = ui.available_height();
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(220.0, height),
                egui::Layout::top_down(egui::Align::Min),
                |ui| self.workflow_list(ui, ctx),
            );
            ui.add_space(12.0);
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), height),
                egui::Layout::top_down(egui::Align::Min),
                |ui| self.run_list(ui, ctx, &project),
            );
        });
    }

    /// The project, on one line, with the other recent ones to switch to.
    fn project_line(&mut self, ui: &mut egui::Ui, ctx: &mut PageCtx, project: &Path) {
        let p = ctx.palette;
        ui.horizontal(|ui| {
            icons::show(ui, Icon::Folder, 16.0, p.accent);
            ui.label(RichText::new(folder_name(project)).strong().color(p.text));
            let others: Vec<&String> = ctx
                .store
                .recent_ci
                .iter()
                .filter(|r| Path::new(r.as_str()) != project)
                .collect();
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if !others.is_empty() {
                    ui.menu_button("Recent", |ui| {
                        for recent in others {
                            if ui.button(recent.as_str()).clicked() {
                                self.set_project(PathBuf::from(recent));
                            }
                        }
                    });
                }
                ui.add(
                    egui::Label::new(
                        RichText::new(project.display().to_string())
                            .size(12.0)
                            .color(p.muted),
                    )
                    .truncate(),
                );
            });
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
        egui::ScrollArea::vertical()
            .id_salt("ci-workflows")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let all_runs = ctx
                    .store
                    .ci
                    .runs
                    .iter()
                    .filter(|r| Some(&r.request.project) == self.project.as_ref())
                    .count();
                if nav_row(
                    ui,
                    p,
                    Icon::Workflow,
                    "All workflows",
                    all_runs,
                    self.selected.is_none(),
                    None,
                )
                .clicked()
                {
                    self.selected = None;
                }
                for workflow in &self.workflows {
                    match workflow {
                        Ok(w) => {
                            if !ctx.matches_search(&[&w.name, &w.file]) {
                                continue;
                            }
                            let runs = ctx
                                .store
                                .ci
                                .runs
                                .iter()
                                .filter(|r| {
                                    Some(&r.request.project) == self.project.as_ref()
                                        && r.request.workflow.file == w.file
                                })
                                .count();
                            let selected = self.selected.as_deref() == Some(w.file.as_str());
                            if nav_row(ui, p, Icon::File, &w.name, runs, selected, None)
                                .on_hover_text(&w.file)
                                .clicked()
                            {
                                self.selected = Some(w.file.clone());
                            }
                        }
                        Err((file, why)) => {
                            let name = file.trim_start_matches(".github/workflows/");
                            nav_row(ui, p, Icon::File, name, 0, false, Some(p.danger))
                                .on_hover_text(why.as_str());
                        }
                    }
                }
                if self.workflows.is_empty() {
                    ui.label(
                        RichText::new("No workflows in .github/workflows")
                            .size(12.0)
                            .color(p.muted),
                    );
                }
            });
    }

    fn run_list(&mut self, ui: &mut egui::Ui, ctx: &mut PageCtx, project: &Path) {
        let p = ctx.palette;
        let chosen: Option<Workflow> = self.selected.as_ref().and_then(|file| {
            self.workflows
                .iter()
                .flatten()
                .find(|w| w.file == *file)
                .cloned()
        });
        if self.selected.is_some() && chosen.is_none() {
            // The file went away.
            self.selected = None;
        }
        // The header: what's shown, and running it.
        ui.horizontal(|ui| {
            match &chosen {
                Some(w) => {
                    ui.label(
                        RichText::new(&w.name)
                            .font(egui::FontId::new(15.0, crate::theme::semibold()))
                            .color(p.text),
                    );
                    let mut about = w.file.trim_start_matches(".github/workflows/").to_owned();
                    if !w.on.is_empty() {
                        about.push_str(&format!(" · on {}", w.on.join(", ")));
                    }
                    ui.add(
                        egui::Label::new(RichText::new(about).size(12.0).color(p.muted)).truncate(),
                    );
                }
                None => {
                    ui.label(
                        RichText::new("All workflows")
                            .font(egui::FontId::new(15.0, crate::theme::semibold()))
                            .color(p.text),
                    );
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let runnable: Vec<Workflow> = match &chosen {
                    Some(w) => vec![w.clone()],
                    None => self.workflows.iter().flatten().cloned().collect(),
                };
                match runnable.as_slice() {
                    [] => {}
                    [one] => {
                        if widgets::primary_button(ui, "Run workflow").clicked() {
                            self.ask_to_run(&one.clone());
                        }
                    }
                    many => {
                        let mut asked = None;
                        ui.menu_button("Run workflow ▾", |ui| {
                            for w in many {
                                if ui.button(&w.name).clicked() {
                                    asked = Some(w.clone());
                                    ui.close();
                                }
                            }
                        });
                        if let Some(w) = asked {
                            self.ask_to_run(&w);
                        }
                    }
                }
            });
        });
        ui.add_space(6.0);
        let runs: Vec<&CiRun> = ctx
            .store
            .ci
            .runs
            .iter()
            .filter(|r| r.request.project == project)
            .filter(|r| {
                chosen
                    .as_ref()
                    .is_none_or(|w| r.request.workflow.file == w.file)
            })
            .filter(|r| ctx.matches_search(&[r.title()]))
            .collect();
        if runs.is_empty() {
            egui::Frame::new()
                .stroke(egui::Stroke::new(1.0, p.border))
                .corner_radius(egui::CornerRadius::same(10))
                .inner_margin(egui::Margin::same(24))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new("No runs yet").strong().color(p.text));
                        ui.label(
                            RichText::new("Run a workflow: its jobs run here in containers, step by step, with live logs.")
                                .size(12.5)
                                .color(p.muted),
                        );
                    });
                });
            return;
        }
        let mut opened = None;
        let mut cancel = None;
        egui::Frame::new()
            .stroke(egui::Stroke::new(1.0, p.border))
            .corner_radius(egui::CornerRadius::same(10))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                egui::ScrollArea::vertical()
                    .id_salt("ci-runs")
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        for (i, run) in runs.iter().enumerate() {
                            if i > 0 {
                                let y = ui.cursor().top();
                                let x = ui.max_rect().x_range();
                                ui.painter().hline(x, y, egui::Stroke::new(1.0, p.border));
                            }
                            let (clicked, stop) = run_row(ui, p, run);
                            if clicked {
                                opened = Some(run.id);
                            }
                            if stop {
                                cancel = Some(run.id);
                            }
                        }
                    });
            });
        if let Some(id) = cancel {
            ctx.request(Intent::Ci(CiOp::Cancel(id)));
        } else if let Some(id) = opened {
            self.open_run(id);
        }
    }
}

/// A row of the workflow list: icon, name, how many runs.
fn nav_row(
    ui: &mut egui::Ui,
    p: &Palette,
    icon: Icon,
    name: &str,
    runs: usize,
    selected: bool,
    tint: Option<egui::Color32>,
) -> egui::Response {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 30.0), egui::Sense::click());
    if selected || response.hovered() {
        let fill = if selected {
            p.raised
        } else {
            p.raised.gamma_multiply(0.6)
        };
        ui.painter()
            .rect_filled(rect, egui::CornerRadius::same(7), fill);
    }
    // The count on the right; the name gets the rest.
    let count_width = if runs > 0 {
        let galley = ui.painter().layout_no_wrap(
            runs.to_string(),
            egui::FontId::proportional(11.5),
            p.muted,
        );
        let at = egui::pos2(
            rect.right() - 10.0 - galley.size().x,
            rect.center().y - galley.size().y / 2.0,
        );
        let w = galley.size().x;
        ui.painter().galley(at, galley, p.muted);
        w + 14.0
    } else {
        0.0
    };
    let inner = egui::Rect::from_min_max(
        rect.min + egui::vec2(8.0, 0.0),
        egui::pos2(rect.right() - 8.0 - count_width, rect.bottom()),
    );
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(inner)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    let color = if selected { p.accent } else { p.muted };
    icons::show(&mut child, icon, 14.0, tint.unwrap_or(color));
    let text = RichText::new(name).color(tint.unwrap_or(p.text));
    let text = if selected { text.strong() } else { text };
    child.add(egui::Label::new(text).truncate().selectable(false));
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A run, on one line: its state, workflow and number, what it ran as,
/// how long it took and when. Returns (opened, asked to cancel).
fn run_row(ui: &mut egui::Ui, p: &Palette, run: &CiRun) -> (bool, bool) {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 40.0), egui::Sense::click());
    if response.hovered() {
        ui.painter().rect_filled(
            rect,
            egui::CornerRadius::same(8),
            p.raised.gamma_multiply(0.6),
        );
    }
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect.shrink2(egui::vec2(12.0, 0.0)))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    let mut stop = false;
    state_mark(&mut child, p, run.state, 16.0);
    child.add_space(4.0);
    child.add(
        egui::Label::new(
            RichText::new(format!("{} #{}", run.title(), run.number))
                .strong()
                .color(p.text),
        )
        .truncate()
        .selectable(false),
    );
    child.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        if !run.state.done() && widgets::icon_button(ui, Icon::Stop, "Cancel the run").clicked() {
            stop = true;
        }
        ui.add_sized(
            egui::vec2(70.0, 20.0),
            egui::Label::new(
                RichText::new(ago(run.started.elapsed()))
                    .size(12.0)
                    .color(p.muted),
            )
            .selectable(false),
        );
        ui.add_sized(
            egui::vec2(60.0, 20.0),
            egui::Label::new(
                RichText::new(duration(run.duration()))
                    .size(12.0)
                    .color(p.muted),
            )
            .selectable(false),
        );
        widgets::pill(ui, &run.request.event, p.muted);
    });
    let opened = response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked();
    (opened && !stop, stop)
}

/// `just now`, `5m ago`, `2h ago`.
fn ago(d: Duration) -> String {
    match d.as_secs() {
        0..60 => "just now".into(),
        s @ 60..3600 => format!("{}m ago", s / 60),
        s @ 3600..86_400 => format!("{}h ago", s / 3600),
        s => format!("{}d ago", s / 86_400),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn says_how_long_ago() {
        assert_eq!(ago(Duration::from_secs(5)), "just now");
        assert_eq!(ago(Duration::from_secs(300)), "5m ago");
        assert_eq!(ago(Duration::from_secs(7200)), "2h ago");
        assert_eq!(ago(Duration::from_secs(3 * 86_400)), "3d ago");
    }
}
