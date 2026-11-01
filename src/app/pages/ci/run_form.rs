//! The Run dialog of a workflow: the event it runs as, its inputs (for a
//! manual run), and the secrets it gets.

use std::path::{Path, PathBuf};

use eframe::egui::{self, CornerRadius, Frame, Margin, RichText, Stroke};

use crate::app::components::widgets;
use crate::docker::ci::{RunRequest, Workflow, image, secrets, workflow::Input};
use crate::theme::Palette;

/// The events offered when a workflow runs on none of these.
const EVENTS: [&str; 3] = ["push", "pull_request", "workflow_dispatch"];

pub struct RunForm {
    project: PathBuf,
    workflow: Workflow,
    event: String,
    events: Vec<String>,
    /// Each input and its value.
    inputs: Vec<(Input, String)>,
    secrets: Vec<(String, String)>,
    /// Show the secrets' values.
    reveal: bool,
}

pub enum Outcome {
    Run(Box<RunRequest>),
    Cancel,
}

impl RunForm {
    pub fn new(project: &Path, workflow: &Workflow) -> Self {
        let mut events: Vec<String> = workflow
            .on
            .iter()
            .filter(|e| EVENTS.contains(&e.as_str()))
            .cloned()
            .collect();
        if events.is_empty() {
            events.push("push".into());
        }
        let mut secrets = secrets::load(project);
        for name in secrets_used(&project.join(&workflow.file)) {
            if !secrets.iter().any(|(k, _)| *k == name) {
                secrets.push((name, String::new()));
            }
        }
        Self {
            project: project.to_owned(),
            event: events[0].clone(),
            events,
            inputs: workflow
                .inputs
                .iter()
                .map(|i| (i.clone(), i.default.clone()))
                .collect(),
            secrets,
            reveal: false,
            workflow: workflow.clone(),
        }
    }

    /// The run to make, or what's missing.
    fn checked(&self) -> Result<RunRequest, String> {
        let manual = self.event == "workflow_dispatch";
        if manual
            && let Some((input, _)) = self
                .inputs
                .iter()
                .find(|(i, v)| i.required && v.trim().is_empty())
        {
            return Err(format!("Fill in {}", input.name));
        }
        Ok(RunRequest {
            run: 0,
            project: self.project.clone(),
            workflow: self.workflow.clone(),
            event: self.event.clone(),
            inputs: if manual {
                self.inputs
                    .iter()
                    .map(|(i, v)| (i.name.clone(), v.clone()))
                    .collect()
            } else {
                Vec::new()
            },
            secrets: self
                .secrets
                .iter()
                .filter(|(k, v)| !k.trim().is_empty() && !v.is_empty())
                .map(|(k, v)| (k.trim().to_owned(), v.clone()))
                .collect(),
        })
    }

    /// Keeps the secrets for next time. Says why when they couldn't be.
    pub fn save_secrets(&self) -> Result<(), String> {
        let kept: Vec<(String, String)> = self
            .secrets
            .iter()
            .filter(|(_, v)| !v.is_empty())
            .cloned()
            .collect();
        secrets::save(&self.project, &kept)
    }

    /// Jobs that can't run here, and why.
    fn skipped(&self) -> Vec<String> {
        self.workflow
            .jobs
            .iter()
            .filter_map(|job| {
                image::ubuntu_for(&job.runs_on)
                    .err()
                    .map(|why| format!("{}: {why}", job.id))
            })
            .collect()
    }
}

/// The names of the secrets a workflow file mentions, in order.
fn secrets_used(file: &Path) -> Vec<String> {
    let text = std::fs::read_to_string(file).unwrap_or_default();
    let mut names: Vec<String> = Vec::new();
    for (at, _) in text.match_indices("secrets.") {
        let name: String = text[at + "secrets.".len()..]
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        if !name.is_empty() && !names.contains(&name) {
            names.push(name);
        }
    }
    names
}

/// Draws the dialog. Returns what the user chose, once they choose.
pub fn show(ui: &mut egui::Ui, p: &Palette, form: &mut RunForm) -> Option<Outcome> {
    let mut outcome = None;
    let modal = egui::Modal::new(egui::Id::unique("ferrodock-ci-run"))
        .frame(
            Frame::new()
                .fill(p.sidebar)
                .stroke(Stroke::new(1.0, p.border))
                .corner_radius(CornerRadius::same(12))
                .inner_margin(Margin::same(20)),
        )
        .show(ui.ctx(), |ui| {
            ui.set_width(520.0);
            ui.label(
                RichText::new(format!("Run {}", form.workflow.name))
                    .font(egui::FontId::new(18.0, crate::theme::semibold()))
                    .color(p.text),
            );
            ui.label(
                RichText::new(format!(
                    "{} · its jobs run in containers on this engine",
                    form.workflow.file
                ))
                .size(12.5)
                .color(p.muted),
            );
            ui.add_space(14.0);
            egui::ScrollArea::vertical()
                .max_height(380.0)
                .auto_shrink([false, true])
                .show(ui, |ui| fields(ui, p, form));
            ui.add_space(10.0);
            let skipped = form.skipped();
            if !skipped.is_empty() {
                ui.label(
                    RichText::new(format!("Skipped here: {}", skipped.join("; ")))
                        .size(12.0)
                        .color(p.warning),
                );
            }
            let checked = form.checked();
            let note = match &checked {
                Err(problem) => RichText::new(problem).color(p.danger),
                Ok(_) => RichText::new(
                    "The first run makes the runner image, which takes a few minutes.",
                )
                .color(p.muted),
            };
            ui.add(egui::Label::new(note.size(12.0)).wrap());
            ui.add_space(10.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let run = ui
                    .add_enabled_ui(checked.is_ok(), |ui| widgets::primary_button(ui, "Run"))
                    .inner;
                if run.clicked()
                    && let Ok(request) = checked
                {
                    outcome = Some(Outcome::Run(Box::new(request)));
                }
                if ui.button("Cancel").clicked() {
                    outcome = Some(Outcome::Cancel);
                }
            });
        });
    if outcome.is_none() && modal.should_close() {
        outcome = Some(Outcome::Cancel);
    }
    outcome
}

fn fields(ui: &mut egui::Ui, p: &Palette, form: &mut RunForm) {
    egui::Grid::new("ci-run-form")
        .num_columns(2)
        .spacing([16.0, 10.0])
        .show(ui, |ui| {
            ui.label(RichText::new("Run as").color(p.muted));
            egui::ComboBox::from_id_salt("ci-event")
                .selected_text(event_label(&form.event))
                .show_ui(ui, |ui| {
                    for event in &form.events {
                        ui.selectable_value(&mut form.event, event.clone(), event_label(event));
                    }
                });
            ui.end_row();
            if form.event == "workflow_dispatch" {
                for (input, value) in &mut form.inputs {
                    let mut name = input.name.clone();
                    if input.required {
                        name.push_str(" *");
                    }
                    let label = ui
                        .label(RichText::new(name).color(p.muted))
                        .on_hover_text(&input.description);
                    match input.kind.as_str() {
                        "boolean" => {
                            let mut on = value == "true";
                            if ui.checkbox(&mut on, &input.description).changed() {
                                *value = on.to_string();
                            }
                        }
                        "choice" => {
                            egui::ComboBox::from_id_salt(("ci-input", &input.name))
                                .selected_text(value.as_str())
                                .show_ui(ui, |ui| {
                                    for option in &input.options {
                                        ui.selectable_value(value, option.clone(), option);
                                    }
                                });
                        }
                        _ => {
                            ui.add(
                                egui::TextEdit::singleline(value)
                                    .hint_text(&input.description)
                                    .desired_width(f32::INFINITY),
                            )
                            .labelled_by(label.id);
                        }
                    }
                    ui.end_row();
                }
            }
        });
    ui.add_space(12.0);
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("SECRETS")
                .font(egui::FontId::new(11.5, crate::theme::semibold()))
                .color(p.muted),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.small_button("Add").clicked() {
                form.secrets.push((String::new(), String::new()));
            }
            ui.checkbox(&mut form.reveal, "Show values");
        });
    });
    let mut remove = None;
    for (i, (name, value)) in form.secrets.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(name)
                    .hint_text("NAME")
                    .font(egui::TextStyle::Monospace)
                    .desired_width(170.0),
            );
            ui.add(
                egui::TextEdit::singleline(value)
                    .hint_text("value")
                    .password(!form.reveal)
                    .desired_width(ui.available_width() - 40.0),
            );
            if widgets::icon_button(ui, crate::app::components::icons::Icon::Close, "Remove")
                .clicked()
            {
                remove = Some(i);
            }
        });
    }
    if let Some(i) = remove {
        form.secrets.remove(i);
    }
    ui.label(
        RichText::new(
            "Kept on this computer only, in a file only you can read, never in the \
             project. They're masked in the logs.",
        )
        .size(12.0)
        .color(p.muted),
    );
}

fn event_label(event: &str) -> &str {
    match event {
        "push" => "A push",
        "pull_request" => "A pull request",
        "workflow_dispatch" => "A manual run",
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_secrets_and_checks_inputs() {
        let dir = std::env::temp_dir().join(format!("ferrodock-ci-form-{}", std::process::id()));
        std::fs::create_dir_all(dir.join(".github/workflows")).unwrap();
        std::fs::write(
            dir.join(".github/workflows/ci.yml"),
            "env:\n  A: ${{ secrets.NPM_TOKEN }}\n  B: ${{ secrets.NPM_TOKEN }} ${{secrets.DEPLOY_KEY}}\n",
        )
        .unwrap();
        assert_eq!(
            secrets_used(&dir.join(".github/workflows/ci.yml")),
            ["NPM_TOKEN", "DEPLOY_KEY"]
        );

        let workflow = Workflow {
            file: ".github/workflows/ci.yml".into(),
            name: "CI".into(),
            on: vec!["workflow_dispatch".into(), "schedule".into()],
            inputs: vec![Input {
                name: "target".into(),
                required: true,
                kind: "string".into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        let mut form = RunForm::new(&dir, &workflow);
        assert_eq!(form.events, ["workflow_dispatch"]);
        assert!(form.checked().is_err());
        form.inputs[0].1 = "prod".into();
        form.secrets[0].1 = "s3cret".into();
        let request = form.checked().unwrap();
        assert_eq!(request.inputs, [("target".to_owned(), "prod".to_owned())]);
        assert_eq!(
            request.secrets,
            [("NPM_TOKEN".to_owned(), "s3cret".to_owned())]
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}
