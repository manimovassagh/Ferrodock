//! The "Run image" dialog: a friendly form for the common `docker run`
//! options, with a live preview of the equivalent command.

use eframe::egui::{self, CornerRadius, Frame, Margin, RichText, Stroke};

use crate::app::components::{inspect, widgets};
use crate::app::store::Store;
use crate::docker::{ContainerInfo, ImageInfo, Inspection, Restart, RunSpec};
use crate::theme::Palette;

/// The form's state while the dialog is open. Numbers are kept as text so
/// half-typed values don't get lost.
pub struct RunForm {
    /// The dialog's title, e.g. "Run nginx:alpine".
    title: String,
    image_id: String,
    image: String,
    name: String,
    ports: Vec<(String, String)>,
    env: Vec<(String, String)>,
    volumes: Vec<(String, String)>,
    network: String,
    restart: Restart,
    command: String,
    /// Whether the image's exposed ports were filled in yet.
    prefilled: bool,
}

pub enum Outcome {
    Run(RunSpec),
    Cancel,
}

impl RunForm {
    /// A copy of a running container's setup, under a new name. Host ports
    /// move up by one so the copy doesn't clash with the original.
    pub fn duplicate(c: &ContainerInfo, i: &Inspection) -> Self {
        let run = &i.run;
        let image = if run.image.is_empty() {
            c.image.clone()
        } else {
            run.image.clone()
        };
        Self {
            title: format!("Duplicate {}", c.name),
            image_id: c.image_id.clone(),
            image,
            name: format!("{}-copy", c.name),
            ports: run
                .ports
                .iter()
                .map(|(host, container)| {
                    (host.saturating_add(1).to_string(), container.to_string())
                })
                .collect(),
            env: i.env.clone(),
            volumes: run.volumes.clone(),
            network: run.network.clone(),
            restart: Restart::from_name(i.restart_policy.as_deref().unwrap_or("no")),
            command: run.command.clone(),
            prefilled: true,
        }
    }

    pub fn new(image: &ImageInfo) -> Self {
        Self {
            title: format!(
                "Run {}",
                if image.tags.is_empty() {
                    image.short_id()
                } else {
                    image.name()
                }
            ),
            image_id: image.id.clone(),
            image: if image.tags.is_empty() {
                image.id.clone()
            } else {
                image.name().to_owned()
            },
            name: String::new(),
            ports: Vec::new(),
            env: Vec::new(),
            volumes: Vec::new(),
            network: String::new(),
            restart: Restart::No,
            command: String::new(),
            prefilled: false,
        }
    }

    /// The image whose inspect output is needed to pre-fill ports.
    pub fn image_id(&self) -> &str {
        &self.image_id
    }

    /// Builds the spec, or says what's wrong with the form.
    fn spec(&self) -> Result<RunSpec, String> {
        let mut ports = Vec::new();
        for (host, container) in &self.ports {
            if host.trim().is_empty() && container.trim().is_empty() {
                continue;
            }
            let parse = |s: &str| s.trim().parse::<u16>().ok().filter(|p| *p > 0);
            match (parse(host), parse(container)) {
                (Some(h), Some(c)) => ports.push((h, c)),
                _ => return Err(format!("“{host} → {container}” isn't a valid port pair")),
            }
        }
        let name = self.name.trim();
        if !name.is_empty()
            && !name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "_.-".contains(c))
        {
            return Err("Names can only use letters, digits, _ . and -".into());
        }
        Ok(RunSpec {
            image: self.image.clone(),
            name: name.to_owned(),
            ports,
            env: self.env.clone(),
            volumes: self.volumes.clone(),
            network: self.network.clone(),
            restart: self.restart,
            command: self.command.clone(),
        })
    }

    /// Fills in the ports the image exposes, once its inspect output is in.
    fn prefill(&mut self, store: &Store) {
        if self.prefilled {
            return;
        }
        let Some(Ok(inspection)) = store
            .inspection
            .as_ref()
            .filter(|_| store.inspecting.as_deref() == Some(self.image_id.as_str()))
        else {
            return;
        };
        for port in &inspection.exposed_ports {
            if let Some(number) = port.strip_suffix("/tcp") {
                self.ports.push((number.to_owned(), number.to_owned()));
            }
        }
        self.prefilled = true;
    }
}

/// Draws the dialog. Returns what the user chose, once they choose.
pub fn show(ui: &mut egui::Ui, p: &Palette, form: &mut RunForm, store: &Store) -> Option<Outcome> {
    form.prefill(store);
    let mut outcome = None;
    let modal = egui::Modal::new(egui::Id::unique("ferrodock-run"))
        .frame(
            Frame::new()
                .fill(p.sidebar)
                .stroke(Stroke::new(1.0, p.border))
                .corner_radius(CornerRadius::same(12))
                .inner_margin(Margin::same(20)),
        )
        .show(ui.ctx(), |ui| {
            ui.set_width(540.0);
            ui.label(
                RichText::new(&form.title)
                    .font(egui::FontId::new(18.0, crate::theme::semibold()))
                    .color(p.text),
            );
            ui.label(
                RichText::new("Creates a new container from this image and starts it.")
                    .color(p.muted),
            );
            ui.add_space(12.0);

            egui::ScrollArea::vertical()
                .max_height(420.0)
                .show(ui, |ui| {
                    egui::Grid::new("run-form")
                        .num_columns(2)
                        .spacing([16.0, 10.0])
                        .show(ui, |ui| {
                            label(ui, p, "Name");
                            ui.add(
                                egui::TextEdit::singleline(&mut form.name)
                                    .hint_text("Docker picks one")
                                    .desired_width(f32::INFINITY),
                            );
                            ui.end_row();

                            label(ui, p, "Ports");
                            pairs(
                                ui,
                                "port",
                                &mut form.ports,
                                ("Host", "Container"),
                                "→",
                                "Add port",
                            );
                            ui.end_row();

                            label(ui, p, "Environment");
                            pairs(
                                ui,
                                "env",
                                &mut form.env,
                                ("KEY", "value"),
                                "=",
                                "Add variable",
                            );
                            ui.end_row();

                            label(ui, p, "Volumes");
                            pairs(
                                ui,
                                "volume",
                                &mut form.volumes,
                                ("Volume or host path", "Path in container"),
                                "→",
                                "Add volume",
                            );
                            ui.end_row();

                            label(ui, p, "Network");
                            let current = if form.network.is_empty() {
                                "Default (bridge)".to_owned()
                            } else {
                                form.network.clone()
                            };
                            egui::ComboBox::from_id_salt("run-network")
                                .selected_text(current)
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(
                                        &mut form.network,
                                        String::new(),
                                        "Default (bridge)",
                                    );
                                    for net in store.networks.iter().filter(|n| n.name != "bridge")
                                    {
                                        ui.selectable_value(
                                            &mut form.network,
                                            net.name.clone(),
                                            &net.name,
                                        );
                                    }
                                });
                            ui.end_row();

                            label(ui, p, "Restart");
                            egui::ComboBox::from_id_salt("run-restart")
                                .selected_text(form.restart.as_str())
                                .show_ui(ui, |ui| {
                                    for policy in Restart::ALL {
                                        ui.selectable_value(
                                            &mut form.restart,
                                            policy,
                                            policy.as_str(),
                                        );
                                    }
                                });
                            ui.end_row();

                            label(ui, p, "Command");
                            ui.add(
                                egui::TextEdit::singleline(&mut form.command)
                                    .hint_text("The image's default")
                                    .desired_width(f32::INFINITY),
                            );
                            ui.end_row();
                        });
                });

            ui.add_space(12.0);
            let spec = form.spec();
            match &spec {
                Ok(spec) => preview(ui, p, &spec.docker_command()),
                Err(problem) => {
                    ui.label(RichText::new(problem).color(p.danger));
                }
            }

            ui.add_space(14.0);
            ui.horizontal(|ui| {
                if !form.prefilled && inspect::needs_loading(store, &form.image_id) {
                    ui.spinner();
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let run =
                        ui.add_enabled_ui(spec.is_ok(), |ui| widgets::primary_button(ui, "Run"));
                    if run.inner.clicked()
                        && let Ok(spec) = spec
                    {
                        outcome = Some(Outcome::Run(spec));
                    }
                    if ui.button("Cancel").clicked() {
                        outcome = Some(Outcome::Cancel);
                    }
                });
            });
        });
    if outcome.is_none() && modal.should_close() {
        outcome = Some(Outcome::Cancel);
    }
    outcome
}

fn label(ui: &mut egui::Ui, p: &Palette, text: &str) {
    ui.label(RichText::new(text).color(p.muted));
}

/// Editable rows of two text fields with a remove button, plus "Add".
fn pairs(
    ui: &mut egui::Ui,
    salt: &str,
    rows: &mut Vec<(String, String)>,
    hints: (&str, &str),
    joiner: &str,
    add_label: &str,
) {
    ui.vertical(|ui| {
        let mut remove = None;
        for (i, (left, right)) in rows.iter_mut().enumerate() {
            ui.push_id((salt, i), |ui| {
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(left)
                            .hint_text(hints.0)
                            .desired_width(150.0),
                    );
                    ui.label(joiner);
                    ui.add(
                        egui::TextEdit::singleline(right)
                            .hint_text(hints.1)
                            .desired_width(150.0),
                    );
                    if widgets::icon_button(
                        ui,
                        crate::app::components::icons::Icon::Close,
                        "Remove",
                    )
                    .clicked()
                    {
                        remove = Some(i);
                    }
                });
            });
        }
        if let Some(i) = remove {
            rows.remove(i);
        }
        if ui.small_button(format!("+ {add_label}")).clicked() {
            rows.push(Default::default());
        }
    });
}

/// The equivalent `docker run` command, with a copy button.
fn preview(ui: &mut egui::Ui, p: &Palette, command: &str) {
    Frame::new()
        .fill(p.bg)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::same(10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("EQUIVALENT COMMAND")
                        .size(11.0)
                        .color(p.muted),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("Copy").clicked() {
                        ui.ctx().copy_text(command.to_owned());
                    }
                });
            });
            ui.add(
                egui::Label::new(RichText::new(command).monospace().size(12.0).color(p.text))
                    .wrap(),
            );
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn form() -> RunForm {
        RunForm::new(&ImageInfo {
            id: "sha256:abc".into(),
            tags: vec!["nginx:alpine".into()],
            size: 0,
            created: 0,
        })
    }

    #[test]
    fn blank_port_rows_are_ignored_and_bad_ones_reported() {
        let mut f = form();
        f.ports = vec![(String::new(), String::new()), ("8080".into(), "80".into())];
        assert_eq!(f.spec().unwrap().ports, vec![(8080, 80)]);
        f.ports.push(("http".into(), "80".into()));
        assert!(f.spec().is_err());
    }

    #[test]
    fn rejects_invalid_names() {
        let mut f = form();
        f.name = "my web".into();
        assert!(f.spec().is_err());
        f.name = "my-web_1".into();
        assert_eq!(f.spec().unwrap().name, "my-web_1");
    }
}
