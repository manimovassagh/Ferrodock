//! One run, the way GitHub shows it: its jobs on the left, and the steps of
//! the chosen job on the right, each opening onto its log.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use eframe::egui::{self, Color32, RichText, Sense, vec2};

use crate::app::components::icons::{self, Icon};
use crate::app::components::{ansi, widgets};
use crate::app::page::{Intent, PageCtx};
use crate::app::store::{CiRun, JobRun, StepRun, elapsed};
use crate::docker::ci::{CiOp, LineKind, State};
use crate::theme::{DARK, Palette};

/// Height of each row of the steps panel.
const ROW: f32 = 20.0;

/// What the user opened and closed in a run.
#[derive(Default)]
pub struct RunView {
    /// The job shown; None picks one (running, else failed, else first).
    pub job: Option<usize>,
    /// Steps opened or closed by hand, by (job, step).
    toggled: HashMap<(usize, usize), bool>,
    /// `::group::`s opened, by (job, step, line).
    groups: HashSet<(usize, usize, usize)>,
}

/// What a click asks for.
pub enum Action {
    Back,
    Rerun,
}

/// A row of the steps panel.
enum Row<'a> {
    Step(usize, &'a StepRun),
    /// A log line, its number, and whether it's in an open group.
    Line {
        step: usize,
        index: usize,
        number: usize,
        nested: bool,
    },
    /// "N earlier lines aren't kept".
    Dropped(usize),
}

impl RunView {
    fn step_open(&self, job: usize, step: usize, s: &StepRun) -> bool {
        self.toggled
            .get(&(job, step))
            .copied()
            .unwrap_or(matches!(s.state, State::Running | State::Failure))
    }

    pub fn show(&mut self, ui: &mut egui::Ui, ctx: &mut PageCtx, run: &CiRun) -> Option<Action> {
        let p = ctx.palette;
        let mut action = None;
        ui.horizontal(|ui| {
            if ui.button("← Runs").clicked() {
                action = Some(Action::Back);
            }
            state_mark(ui, p, run.state, 18.0);
            ui.label(
                RichText::new(format!("{} #{}", run.title(), run.number))
                    .font(egui::FontId::new(16.0, crate::theme::semibold()))
                    .color(p.text),
            );
            ui.label(
                RichText::new(format!(
                    "{} · {} · {}",
                    run.request.event,
                    run.state.label(),
                    duration(run.duration())
                ))
                .size(12.5)
                .color(p.muted),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if run.state.done() {
                    if widgets::button(ui, Icon::Restart, "Re-run").clicked() {
                        action = Some(Action::Rerun);
                    }
                } else if widgets::danger_button(ui, p, "Cancel").clicked() {
                    ctx.request(Intent::Ci(CiOp::Cancel(run.id)));
                }
            });
        });
        ui.add_space(10.0);

        let job = self.job.filter(|j| *j < run.jobs.len()).or_else(|| {
            let pick = |want: State| run.jobs.iter().position(|j| j.state == want);
            pick(State::Running)
                .or_else(|| pick(State::Failure))
                .or((!run.jobs.is_empty()).then_some(0))
        });
        let height = ui.available_height();
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(
                vec2(230.0, height),
                egui::Layout::top_down(egui::Align::Min),
                |ui| self.jobs(ui, ctx, run, job),
            );
            ui.add_space(8.0);
            ui.allocate_ui_with_layout(
                vec2(ui.available_width(), height),
                egui::Layout::top_down(egui::Align::Min),
                |ui| match job {
                    Some(j) => self.steps(ui, run, j),
                    None => {
                        ui.add_space(20.0);
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(RichText::new("Starting…").color(p.muted));
                        });
                    }
                },
            );
        });
        action
    }

    fn jobs(&mut self, ui: &mut egui::Ui, ctx: &mut PageCtx, run: &CiRun, shown: Option<usize>) {
        let p = ctx.palette;
        egui::ScrollArea::vertical()
            .id_salt("ci-jobs")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                heading(ui, p, "JOBS");
                for (i, job) in run.jobs.iter().enumerate() {
                    if job.name.is_empty() {
                        continue;
                    }
                    if job_row(ui, p, job, shown == Some(i)).clicked() {
                        self.job = Some(i);
                    }
                }
                if !run.artifacts.is_empty() {
                    ui.add_space(14.0);
                    heading(ui, p, "ARTIFACTS");
                    for name in &run.artifacts {
                        ui.horizontal(|ui| {
                            icons::show(ui, Icon::File, 14.0, p.muted);
                            ui.add(egui::Label::new(RichText::new(name).color(p.text)).truncate());
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if widgets::icon_button(ui, Icon::Download, "Save to Downloads")
                                        .clicked()
                                    {
                                        ctx.request(Intent::Ci(CiOp::SaveArtifact {
                                            run: run.id,
                                            name: name.clone(),
                                        }));
                                    }
                                },
                            );
                        });
                    }
                }
            });
    }

    fn steps(&mut self, ui: &mut egui::Ui, run: &CiRun, j: usize) {
        let job = &run.jobs[j];
        let rows = self.rows(j, job);
        let log = &DARK;
        egui::Frame::new()
            .fill(Color32::from_rgb(0x16, 0x16, 0x19))
            .corner_radius(egui::CornerRadius::same(10))
            .inner_margin(egui::Margin::same(12))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.set_min_height(ui.available_height());
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(&job.name)
                            .font(egui::FontId::new(15.0, crate::theme::semibold()))
                            .color(log.text),
                    );
                    if let Some(d) = elapsed(job.started, job.ended) {
                        ui.label(
                            RichText::new(format!("{} {}", job.state.label(), duration(d)))
                                .size(12.0)
                                .color(log.muted),
                        );
                    }
                });
                ui.add_space(6.0);
                let font = egui::FontId::monospace(12.0);
                let mut clicked_step = None;
                let mut clicked_group = None;
                egui::ScrollArea::both()
                    .id_salt(("ci-steps", run.id, j))
                    .auto_shrink([false, false])
                    .stick_to_bottom(!run.state.done())
                    .show_rows(ui, ROW, rows.len(), |ui, range| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        ui.spacing_mut().interact_size.y = ROW;
                        for row in &rows[range] {
                            match row {
                                Row::Step(i, step) => {
                                    let open = self.step_open(j, *i, step);
                                    if step_row(ui, log, step, open).clicked() {
                                        clicked_step = Some((*i, open));
                                    }
                                }
                                Row::Dropped(n) => {
                                    ui.label(
                                        RichText::new(format!(
                                            "      {n} earlier lines aren't kept"
                                        ))
                                        .font(font.clone())
                                        .color(log.muted),
                                    );
                                }
                                Row::Line {
                                    step,
                                    index,
                                    number,
                                    nested,
                                } => {
                                    let line = &job.steps[*step].lines[*index];
                                    let open = self.groups.contains(&(j, *step, *index));
                                    if log_line(ui, log, &font, line, *number, *nested, open)
                                        .clicked()
                                        && line.kind == LineKind::Group
                                    {
                                        clicked_group = Some((j, *step, *index));
                                    }
                                }
                            }
                        }
                    });
                if let Some((i, open)) = clicked_step {
                    self.toggled.insert((j, i), !open);
                }
                if let Some(key) = clicked_group
                    && !self.groups.remove(&key)
                {
                    self.groups.insert(key);
                }
            });
    }

    /// The rows shown for a job: every step, and the log of open ones
    /// (closed groups folded).
    fn rows<'a>(&self, j: usize, job: &'a JobRun) -> Vec<Row<'a>> {
        let mut rows = Vec::new();
        for (i, step) in job.steps.iter().enumerate() {
            if step.name.is_empty() {
                continue;
            }
            rows.push(Row::Step(i, step));
            if !self.step_open(j, i, step) {
                continue;
            }
            if step.dropped > 0 {
                rows.push(Row::Dropped(step.dropped));
            }
            // None outside a group; Some(open) inside one.
            let mut group: Option<bool> = None;
            for (index, line) in step.lines.iter().enumerate() {
                let number = step.dropped + index + 1;
                match line.kind {
                    LineKind::Group => {
                        let open = self.groups.contains(&(j, i, index));
                        group = Some(open);
                        rows.push(Row::Line {
                            step: i,
                            index,
                            number,
                            nested: false,
                        });
                    }
                    LineKind::EndGroup => group = None,
                    _ => match group {
                        Some(false) => {}
                        Some(true) => rows.push(Row::Line {
                            step: i,
                            index,
                            number,
                            nested: true,
                        }),
                        None => rows.push(Row::Line {
                            step: i,
                            index,
                            number,
                            nested: false,
                        }),
                    },
                }
            }
        }
        rows
    }
}

fn heading(ui: &mut egui::Ui, p: &Palette, text: &str) {
    ui.label(
        RichText::new(text)
            .font(egui::FontId::new(11.5, crate::theme::semibold()))
            .color(p.muted),
    );
    ui.add_space(4.0);
}

fn job_row(ui: &mut egui::Ui, p: &Palette, job: &JobRun, selected: bool) -> egui::Response {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(vec2(width, 32.0), Sense::click());
    if selected || response.hovered() {
        let fill = if selected {
            p.raised
        } else {
            p.raised.gamma_multiply(0.6)
        };
        ui.painter()
            .rect_filled(rect, egui::CornerRadius::same(7), fill);
    }
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect.shrink2(vec2(8.0, 0.0)))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    state_mark(&mut child, p, job.state, 15.0);
    child.add(
        egui::Label::new(RichText::new(&job.name).color(p.text))
            .truncate()
            .selectable(false),
    );
    if let Some(d) = elapsed(job.started, job.ended).filter(|_| job.state != State::Skipped) {
        child.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(RichText::new(duration(d)).size(11.5).color(p.muted));
        });
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn step_row(ui: &mut egui::Ui, p: &Palette, step: &StepRun, open: bool) -> egui::Response {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(vec2(width, ROW), Sense::click());
    if response.hovered() {
        ui.painter()
            .rect_filled(rect, egui::CornerRadius::same(4), p.raised);
    }
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect.shrink2(vec2(4.0, 0.0)))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    let chevron = if open {
        Icon::ChevronDown
    } else {
        Icon::ChevronRight
    };
    icons::show(&mut child, chevron, 12.0, p.muted);
    state_mark(&mut child, p, step.state, 14.0);
    let color = if step.state == State::Skipped {
        p.muted
    } else {
        p.text
    };
    child.add(
        egui::Label::new(RichText::new(&step.name).color(color))
            .truncate()
            .selectable(false),
    );
    if let Some(d) = elapsed(step.started, step.ended).filter(|_| step.state != State::Skipped) {
        child.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(RichText::new(duration(d)).size(11.5).color(p.muted));
        });
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn log_line(
    ui: &mut egui::Ui,
    p: &Palette,
    font: &egui::FontId,
    line: &crate::docker::ci::LogLine,
    number: usize,
    nested: bool,
    group_open: bool,
) -> egui::Response {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        ui.add_sized(
            vec2(44.0, ROW),
            egui::Label::new(
                RichText::new(number.to_string())
                    .font(font.clone())
                    .color(p.muted.gamma_multiply(0.7)),
            )
            .selectable(false),
        );
        if nested {
            ui.add_space(14.0);
        }
        let (prefix, color) = match line.kind {
            LineKind::Group => (if group_open { "▾ " } else { "▸ " }, Some(p.text)),
            LineKind::Error => ("Error: ", Some(p.danger)),
            LineKind::Warning => ("Warning: ", Some(p.warning)),
            LineKind::Notice => ("Notice: ", Some(p.glow)),
            LineKind::Info => ("", Some(p.muted)),
            LineKind::Output | LineKind::EndGroup => ("", None),
        };
        let response = match color {
            Some(color) => ui.add(
                egui::Label::new(
                    RichText::new(format!("{prefix}{}", ansi::strip(&line.text)))
                        .font(font.clone())
                        .color(color),
                )
                .extend()
                .sense(if line.kind == LineKind::Group {
                    Sense::click()
                } else {
                    Sense::hover()
                }),
            ),
            None => {
                ui.add(egui::Label::new(ansi::layout(p, &line.text, "", font.clone())).extend())
            }
        };
        if line.kind == LineKind::Group {
            response.on_hover_cursor(egui::CursorIcon::PointingHand)
        } else {
            response
        }
    })
    .inner
}

/// A round mark for a state: a tick, a cross, a spinner…
pub fn state_mark(ui: &mut egui::Ui, p: &Palette, state: State, size: f32) {
    if state == State::Running {
        ui.add(egui::Spinner::new().size(size).color(p.warning));
        return;
    }
    let (rect, response) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
    response.on_hover_text(state.label());
    let painter = ui.painter();
    let c = rect.center();
    let r = size / 2.0 - 0.5;
    let stroke = |color: Color32| egui::Stroke::new((size / 9.0).max(1.4), color);
    let at = |x: f32, y: f32| c + vec2(x, y) * r;
    match state {
        State::Success => {
            painter.circle_filled(c, r, p.success);
            painter.line(
                vec![at(-0.45, 0.0), at(-0.1, 0.35), at(0.45, -0.3)],
                stroke(Color32::WHITE),
            );
        }
        State::Failure => {
            painter.circle_filled(c, r, p.danger);
            painter.line_segment([at(-0.35, -0.35), at(0.35, 0.35)], stroke(Color32::WHITE));
            painter.line_segment([at(0.35, -0.35), at(-0.35, 0.35)], stroke(Color32::WHITE));
        }
        State::Cancelled => {
            painter.circle_stroke(c, r - 0.5, stroke(p.muted));
            painter.rect_filled(
                egui::Rect::from_center_size(c, vec2(r * 0.8, r * 0.8)),
                1.0,
                p.muted,
            );
        }
        State::Skipped => {
            painter.circle_stroke(c, r - 0.5, stroke(p.muted));
            painter.line_segment([at(-0.5, 0.5), at(0.5, -0.5)], stroke(p.muted));
        }
        State::Queued | State::Running => {
            painter.circle_stroke(c, r - 0.5, stroke(p.muted));
        }
    }
}

/// `4s`, `1m 05s`, `1h 02m`.
pub fn duration(d: Duration) -> String {
    let s = d.as_secs();
    match s {
        0..60 => format!("{s}s"),
        60..3600 => format!("{}m {:02}s", s / 60, s % 60),
        _ => format!("{}h {:02}m", s / 3600, s % 3600 / 60),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::docker::ci::LogLine;

    fn line(text: &str, kind: LineKind) -> LogLine {
        LogLine {
            text: text.into(),
            kind,
        }
    }

    #[test]
    fn folds_groups_and_closed_steps() {
        let job = JobRun {
            name: "test".into(),
            state: State::Running,
            steps: vec![
                StepRun {
                    name: "Set up job".into(),
                    state: State::Success,
                    lines: [line("ok", LineKind::Info)].into(),
                    ..Default::default()
                },
                StepRun {
                    name: "Build".into(),
                    state: State::Running,
                    lines: [
                        line("Installing", LineKind::Group),
                        line("a", LineKind::Output),
                        line("", LineKind::EndGroup),
                        line("done", LineKind::Output),
                    ]
                    .into(),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let mut view = RunView::default();
        // The finished step is closed; the running one is open, its group
        // folded.
        assert_eq!(view.rows(0, &job).len(), 2 + 2);
        view.groups.insert((0, 1, 0));
        assert_eq!(view.rows(0, &job).len(), 2 + 3);
        view.toggled.insert((0, 0), true);
        assert_eq!(view.rows(0, &job).len(), 3 + 3);

        assert_eq!(duration(Duration::from_secs(4)), "4s");
        assert_eq!(duration(Duration::from_secs(65)), "1m 05s");
        assert_eq!(duration(Duration::from_secs(3720)), "1h 02m");
    }
}
