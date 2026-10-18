//! The Run dialog of a template: name, version, ports and the service's
//! own settings, with the connection string that results.

use eframe::egui::{self, CornerRadius, Frame, Margin, RichText, Stroke};

use crate::app::components::widgets;
use crate::app::page::PageCtx;
use crate::app::store::Store;
use crate::docker::{Choices, RunSpec, Template};
use crate::theme::Palette;

/// The dialog's state. Ports are kept as text so half-typed values
/// survive.
pub struct TemplateForm {
    template: &'static Template,
    choices: Choices,
    ports: Vec<String>,
}

pub enum Outcome {
    Run(Box<RunSpec>),
    Cancel,
}

impl TemplateForm {
    /// Filled in with a free name and free ports.
    pub fn new(template: &'static Template, store: &Store) -> Self {
        Self::with(template, super::defaults(store, template))
    }

    /// Filled in with the template's own defaults (nothing loaded to check
    /// against yet).
    pub fn without_store(template: &'static Template) -> Self {
        Self::with(template, template.defaults(&[], |_| true))
    }

    fn with(template: &'static Template, choices: Choices) -> Self {
        let ports = choices.ports.iter().map(u16::to_string).collect();
        Self {
            template,
            choices,
            ports,
        }
    }

    /// The choices, or what's wrong with them.
    fn checked(&self) -> Result<Choices, String> {
        let mut choices = self.choices.clone();
        choices.name = choices.name.trim().to_owned();
        if choices.name.is_empty()
            || !choices
                .name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "_.-".contains(c))
        {
            return Err("Names can only use letters, digits, _ . and -".into());
        }
        choices.ports = Vec::new();
        for text in &self.ports {
            match text.trim().parse::<u16>() {
                Ok(port) if port > 0 => choices.ports.push(port),
                _ => return Err(format!("“{text}” isn't a port number")),
            }
        }
        Ok(choices)
    }
}

/// Draws the dialog. Returns what the user chose, once they choose.
pub fn show(ui: &mut egui::Ui, ctx: &PageCtx, form: &mut TemplateForm) -> Option<Outcome> {
    let p = ctx.palette;
    let template = form.template;
    let mut outcome = None;
    let modal = egui::Modal::new(egui::Id::unique("ferrodock-quick-start"))
        .frame(
            Frame::new()
                .fill(p.sidebar)
                .stroke(Stroke::new(1.0, p.border))
                .corner_radius(CornerRadius::same(12))
                .inner_margin(Margin::same(20)),
        )
        .show(ui.ctx(), |ui| {
            ui.set_width(520.0);
            ui.horizontal(|ui| {
                super::badge(ui, template, 40.0);
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new(format!("Run {}", template.name))
                            .font(egui::FontId::new(18.0, crate::theme::semibold()))
                            .color(p.text),
                    );
                    ui.label(
                        RichText::new(template.reference(&form.choices))
                            .monospace()
                            .size(12.0)
                            .color(p.muted),
                    );
                });
            });
            ui.add_space(14.0);
            fields(ui, p, form, ctx.store);
            ui.add_space(12.0);

            let checked = form.checked();
            if let Ok(choices) = &checked {
                connection(ui, p, &template.connection(choices));
                ui.add_space(12.0);
            }
            ui.horizontal(|ui| {
                match &checked {
                    Err(e) => {
                        ui.label(RichText::new(e).size(12.0).color(p.danger));
                    }
                    Ok(_) => {
                        ui.label(
                            RichText::new(
                                "For local development. Pulls the image if it isn't here.",
                            )
                            .size(12.0)
                            .color(p.muted),
                        );
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let run = ui
                        .add_enabled_ui(checked.is_ok(), |ui| widgets::primary_button(ui, "Run"))
                        .inner;
                    if run.clicked()
                        && let Ok(choices) = &checked
                    {
                        outcome = Some(Outcome::Run(Box::new(template.spec(choices))));
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

fn label(ui: &mut egui::Ui, p: &Palette, text: &str) -> egui::Response {
    ui.label(RichText::new(text).color(p.muted))
}

/// The form's rows: name, version, ports, the service's settings and
/// whether to keep its data.
fn fields(ui: &mut egui::Ui, p: &Palette, form: &mut TemplateForm, store: &Store) {
    let template = form.template;
    egui::Grid::new("quick-start-form")
        .num_columns(2)
        .spacing([16.0, 10.0])
        .show(ui, |ui| {
            let name = label(ui, p, "Name");
            ui.add(egui::TextEdit::singleline(&mut form.choices.name).desired_width(f32::INFINITY))
                .labelled_by(name.id);
            ui.end_row();

            label(ui, p, "Version");
            egui::ComboBox::from_id_salt("quick-start-tag")
                .selected_text(&form.choices.tag)
                .show_ui(ui, |ui| {
                    for tag in template.tags {
                        ui.selectable_value(&mut form.choices.tag, (*tag).to_owned(), *tag);
                    }
                });
            ui.end_row();

            for (port, text) in template.ports.iter().zip(&mut form.ports) {
                let title = if port.label.is_empty() {
                    "Port on your machine".to_owned()
                } else {
                    format!("{} port", port.label)
                };
                let row = label(ui, p, &title);
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(text).desired_width(80.0))
                        .labelled_by(row.id);
                    ui.label(
                        RichText::new(format!("→ {} in the container", port.container))
                            .size(12.0)
                            .color(p.muted),
                    );
                    let taken = text
                        .trim()
                        .parse::<u16>()
                        .is_ok_and(|n| !super::port_free(store, n));
                    if taken {
                        ui.label(RichText::new("in use").size(12.0).color(p.warning));
                    }
                });
                ui.end_row();
            }

            for (field, value) in template.fields.iter().zip(&mut form.choices.values) {
                let row = label(ui, p, field.label);
                ui.add(egui::TextEdit::singleline(value).desired_width(f32::INFINITY))
                    .labelled_by(row.id);
                ui.end_row();
            }

            if template.data.is_some() {
                ui.label("");
                ui.checkbox(
                    &mut form.choices.keep_data,
                    format!(
                        "Keep data in the volume “{}-data”",
                        form.choices.name.trim()
                    ),
                );
                ui.end_row();
            }
        });
}

/// The connection string, with Copy.
fn connection(ui: &mut egui::Ui, p: &Palette, text: &str) {
    Frame::new()
        .fill(p.bg)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::symmetric(12, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new("CONNECTION").size(11.0).color(p.muted));
                    ui.label(RichText::new(text).monospace().color(p.text));
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Copy").clicked() {
                        ui.ctx().copy_text(text.to_owned());
                    }
                });
            });
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checks_name_and_ports() {
        let postgres = crate::docker::template("postgres").unwrap();
        let mut form = TemplateForm::without_store(postgres);
        assert_eq!(form.checked().unwrap().ports, [5432]);
        form.ports[0] = "54x".into();
        assert!(form.checked().is_err());
        form.ports[0] = "6000".into();
        form.choices.name = "my db".into();
        assert!(form.checked().is_err());
        form.choices.name = " db ".into();
        assert_eq!(form.checked().unwrap().name, "db");
    }
}
