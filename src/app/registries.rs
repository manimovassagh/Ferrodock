//! Settings → Registries: sign in to registries and out again. Logins are
//! checked with the registry and kept where the Docker CLI keeps them, so
//! pushes and private pulls work here, in the CLI and in Compose alike.

use eframe::egui::{self, RichText};

use crate::docker::{Login, registry_key};
use crate::theme::Palette;
use crate::worker::RegistryOp;

#[derive(Default)]
pub struct Registries {
    /// Who's signed in where, once read.
    pub logins: Option<Vec<Login>>,
    /// The sign-in form: empty registry means Docker Hub.
    registry: String,
    username: String,
    password: String,
    /// A request is with the worker.
    busy: bool,
    /// The last outcome to show, and whether it's an error.
    message: Option<(String, bool)>,
    /// What to ask the worker, taken by the app.
    pub request: Option<RegistryOp>,
}

impl Registries {
    /// The worker's answer: the logins now, and how the request went.
    pub fn answer(&mut self, logins: Vec<Login>, outcome: Option<Result<String, String>>) {
        self.logins = Some(logins);
        self.busy = false;
        if let Some(outcome) = outcome {
            if outcome.is_ok() {
                self.password.clear();
            }
            self.message = Some(match outcome {
                Ok(text) => (text, false),
                Err(text) => (text, true),
            });
        }
    }

    fn ask(&mut self, op: RegistryOp) {
        self.busy = !matches!(op, RegistryOp::List);
        self.message = None;
        self.request = Some(op);
    }

    /// Draws the section; reads the logins the first time.
    pub fn show(&mut self, ui: &mut egui::Ui, p: &Palette) {
        if self.logins.is_none() && self.request.is_none() {
            self.request = Some(RegistryOp::List);
        }
        ui.label(
            RichText::new("Sign in to push your images and pull private ones.")
                .size(12.0)
                .color(p.muted),
        );
        ui.add_space(6.0);
        let mut sign_out = None;
        for login in self.logins.iter().flatten() {
            ui.horizontal(|ui| {
                crate::app::components::widgets::dot(ui, p.success);
                ui.label(RichText::new(login.label()).color(p.text));
                if !login.username.is_empty() {
                    ui.label(RichText::new(&login.username).color(p.muted));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add_enabled(!self.busy, egui::Button::new("Sign out"))
                        .clicked()
                    {
                        sign_out = Some(login.registry.clone());
                    }
                });
            });
        }
        if let Some(registry) = sign_out {
            self.ask(RegistryOp::SignOut(registry));
        }
        ui.add_space(6.0);
        egui::Grid::new("registry-sign-in")
            .num_columns(2)
            .spacing([18.0, 8.0])
            .show(ui, |ui| {
                ui.label(RichText::new("Registry").color(p.muted));
                ui.add(
                    egui::TextEdit::singleline(&mut self.registry)
                        .hint_text("Docker Hub, or e.g. ghcr.io")
                        .desired_width(f32::INFINITY),
                );
                ui.end_row();
                ui.label(RichText::new("Username").color(p.muted));
                ui.add(egui::TextEdit::singleline(&mut self.username).desired_width(f32::INFINITY));
                ui.end_row();
                ui.label(RichText::new("Password").color(p.muted));
                ui.add(
                    egui::TextEdit::singleline(&mut self.password)
                        .password(true)
                        .hint_text("or an access token")
                        .desired_width(f32::INFINITY),
                );
                ui.end_row();
            });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let ready = !self.busy && !self.username.trim().is_empty() && !self.password.is_empty();
            if ui
                .add_enabled(ready, egui::Button::new("Sign in"))
                .clicked()
            {
                self.ask(RegistryOp::SignIn {
                    registry: registry_key(&self.registry),
                    username: self.username.trim().to_owned(),
                    password: self.password.clone(),
                });
            }
            if self.busy {
                ui.spinner();
            }
        });
        if let Some((text, is_error)) = &self.message {
            ui.label(RichText::new(text).size(12.0).color(if *is_error {
                p.danger
            } else {
                p.muted
            }));
        }
    }
}
