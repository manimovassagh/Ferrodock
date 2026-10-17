//! The Inspect view: environment, labels, restart policy and health, then
//! the raw `docker inspect` JSON.

use eframe::egui::{self, RichText};

use super::details;
use crate::app::store::Store;
use crate::docker::{Inspection, looks_secret};
use crate::theme::Palette;

/// Whether the inspect output of `id` still has to be asked for.
pub fn needs_loading(store: &Store, id: &str) -> bool {
    store.inspecting.as_deref() != Some(id)
}

/// Draws the inspect output of `id` from the store, or a spinner while
/// it loads.
pub fn show(ui: &mut egui::Ui, p: &Palette, store: &Store, id: &str) {
    let result = store
        .inspection
        .as_ref()
        .filter(|_| store.inspecting.as_deref() == Some(id));
    match result {
        None => {
            ui.add_space(20.0);
            ui.vertical_centered(|ui| ui.spinner());
        }
        Some(Err(message)) => {
            ui.label(RichText::new(message).color(p.danger));
        }
        Some(Ok(inspection)) => sections(ui, p, inspection),
    }
}

fn sections(ui: &mut egui::Ui, p: &Palette, i: &Inspection) {
    if i.restart_policy.is_some() || i.health.is_some() {
        details::section(ui, p, "Runtime", |ui| {
            if let Some(policy) = &i.restart_policy {
                details::field(ui, p, "Restart", policy);
            }
            if let Some(health) = &i.health {
                let color = match health.status.as_str() {
                    "healthy" => p.success,
                    "unhealthy" => p.danger,
                    _ => p.warning,
                };
                ui.label(RichText::new("Health").color(p.muted));
                ui.label(RichText::new(&health.status).color(color));
                ui.end_row();
                if health.failing_streak > 0 {
                    details::field(
                        ui,
                        p,
                        "Failing",
                        &format!("{} checks in a row", health.failing_streak),
                    );
                }
                if !health.last_output.is_empty() {
                    details::field(ui, p, "Last check", &health.last_output);
                }
            }
        });
    }

    if !i.env.is_empty() {
        let reveal_id = ui.scope_id().with(("reveal-env", &i.id));
        let mut reveal = ui.data(|d| d.get_temp::<bool>(reveal_id).unwrap_or(false));
        ui.horizontal(|ui| {
            ui.label(RichText::new("ENVIRONMENT").size(11.0).color(p.muted));
            if i.env.iter().any(|(k, _)| looks_secret(k)) {
                ui.checkbox(&mut reveal, RichText::new("Show secrets").size(12.0));
            }
        });
        ui.data_mut(|d| d.insert_temp(reveal_id, reveal));
        key_values(ui, p, "env", &i.env, |key| !reveal && looks_secret(key));
    }

    if !i.labels.is_empty() {
        ui.label(RichText::new("LABELS").size(11.0).color(p.muted));
        key_values(ui, p, "labels", &i.labels, |_| false);
    }

    egui::CollapsingHeader::new(RichText::new("Raw JSON").color(p.muted))
        .id_salt(("inspect-json", &i.id))
        .show(ui, |ui| {
            if ui.small_button("Copy JSON").clicked() {
                ui.ctx().copy_text(i.json.clone());
            }
            ui.add(
                egui::Label::new(RichText::new(&i.json).monospace().size(11.5).color(p.text))
                    .wrap_mode(egui::TextWrapMode::Extend),
            );
        });
}

/// A card of `key  value` rows. Values for which `hide` says so show as
/// dots. Clicking a row copies `KEY=value`.
fn key_values(
    ui: &mut egui::Ui,
    p: &Palette,
    salt: &str,
    rows: &[(String, String)],
    hide: impl Fn(&str) -> bool,
) {
    egui::Frame::new()
        .fill(p.bg)
        .stroke(egui::Stroke::new(1.0, p.border))
        .corner_radius(egui::CornerRadius::same(8))
        .inner_margin(egui::Margin::same(10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            egui::Grid::new(salt)
                .num_columns(2)
                .spacing([12.0, 4.0])
                .show(ui, |ui| {
                    for (key, value) in rows {
                        ui.label(RichText::new(key).monospace().size(12.0).color(p.muted));
                        let shown = if hide(key) {
                            "••••••••"
                        } else {
                            value.as_str()
                        };
                        let response = ui
                            .add(
                                egui::Label::new(
                                    RichText::new(shown).monospace().size(12.0).color(p.text),
                                )
                                .truncate()
                                .sense(egui::Sense::click()),
                            )
                            .on_hover_text("Click to copy");
                        if response.clicked() {
                            ui.ctx().copy_text(format!("{key}={value}"));
                        }
                        ui.end_row();
                    }
                });
        });
    ui.add_space(14.0);
}
