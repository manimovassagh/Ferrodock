//! User preferences: what they are, how they're applied, and the dialog
//! that changes them. They're saved with the window state between runs.

use eframe::egui::{self, CornerRadius, Frame, Margin, RichText, Stroke, ThemePreference};
use serde::{Deserialize, Serialize};

use super::components::{table, widgets};
use crate::theme::Palette;
use crate::update::{Status, Updater};

/// Key the settings are stored under.
pub const STORAGE_KEY: &str = "settings";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Theme {
    #[default]
    Light,
    Dark,
    /// Follow the operating system.
    System,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Density {
    #[default]
    Comfortable,
    /// One-line rows, so more fit on screen.
    Compact,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub theme: Theme,
    pub density: Density,
    /// Interface size, 0.9 to 1.25.
    pub scale: f32,
    /// Look for a new version when Ferrodock starts.
    pub check_updates: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: Theme::default(),
            density: Density::default(),
            scale: 1.0,
            check_updates: true,
        }
    }
}

impl Settings {
    pub const MIN_SCALE: f32 = 0.9;
    pub const MAX_SCALE: f32 = 1.25;

    /// Makes the settings take effect.
    pub fn apply(&self, ctx: &egui::Context) {
        ctx.set_theme(match self.theme {
            Theme::Light => ThemePreference::Light,
            Theme::Dark => ThemePreference::Dark,
            Theme::System => ThemePreference::System,
        });
        ctx.set_zoom_factor(self.scale.clamp(Self::MIN_SCALE, Self::MAX_SCALE));
        table::set_compact(ctx, self.density == Density::Compact);
    }
}

/// Draws the Settings dialog. Changes apply immediately. Returns false
/// once it should close.
pub fn show(ui: &mut egui::Ui, p: &Palette, settings: &mut Settings, updater: &Updater) -> bool {
    let before = settings.clone();
    let mut close = false;
    let modal = egui::Modal::new(egui::Id::unique("ferrodock-settings"))
        .frame(
            Frame::new()
                .fill(p.sidebar)
                .stroke(Stroke::new(1.0, p.border))
                .corner_radius(CornerRadius::same(10))
                .inner_margin(Margin::same(20)),
        )
        .show(ui.ctx(), |ui| {
            ui.set_width(420.0);
            ui.label(RichText::new("Settings").size(18.0).strong().color(p.text));
            ui.add_space(14.0);

            heading(ui, p, "Appearance");
            egui::Grid::new("settings-grid")
                .num_columns(2)
                .spacing([18.0, 12.0])
                .show(ui, |ui| {
                    ui.label(RichText::new("Theme").color(p.muted));
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut settings.theme, Theme::Light, "Light");
                        ui.selectable_value(&mut settings.theme, Theme::Dark, "Dark");
                        ui.selectable_value(&mut settings.theme, Theme::System, "System");
                    });
                    ui.end_row();

                    ui.label(RichText::new("Density").color(p.muted));
                    ui.horizontal(|ui| {
                        ui.selectable_value(
                            &mut settings.density,
                            Density::Comfortable,
                            "Comfortable",
                        );
                        ui.selectable_value(&mut settings.density, Density::Compact, "Compact");
                    });
                    ui.end_row();

                    ui.label(RichText::new("Interface size").color(p.muted));
                    // The slider shows percent; the setting is a factor.
                    let mut percent = (settings.scale * 100.0).round();
                    let slider = egui::Slider::new(
                        &mut percent,
                        Settings::MIN_SCALE * 100.0..=Settings::MAX_SCALE * 100.0,
                    )
                    .step_by(5.0)
                    .suffix("%");
                    // Apply when the drag ends, so the slider doesn't move
                    // under the pointer while the whole window rescales.
                    let response = ui.add(slider);
                    if response.drag_stopped() || (response.changed() && !response.dragged()) {
                        settings.scale = percent / 100.0;
                    }
                    ui.end_row();
                });

            ui.add_space(18.0);
            heading(ui, p, "Updates");
            ui.checkbox(
                &mut settings.check_updates,
                "Check for updates when Ferrodock starts",
            );
            ui.horizontal(|ui| {
                let status = updater.status();
                let (text, color) = match &status {
                    Status::Idle => (String::new(), p.muted),
                    Status::Checking => ("Checking…".into(), p.muted),
                    Status::UpToDate => ("Ferrodock is up to date.".into(), p.muted),
                    Status::Available(a) => {
                        (format!("Version {} is available.", a.version), p.text)
                    }
                    Status::Downloading { version, .. } => {
                        (format!("Downloading {version}…"), p.muted)
                    }
                    Status::Ready { version, .. } => (
                        format!("Version {version} is installed. Restart to use it."),
                        p.text,
                    ),
                    Status::Failed(e) => (e.clone(), p.danger),
                };
                if let Status::Available(available) = status {
                    if widgets::primary_button(ui, "Update now").clicked() {
                        updater.install(available);
                    }
                } else if ui.button("Check now").clicked() {
                    updater.check();
                }
                ui.label(RichText::new(text).color(color));
            });
            if crate::update::installed_with_homebrew() {
                ui.label(
                    RichText::new(
                        "Installed with Homebrew: `brew upgrade --cask ferrodock` works too.",
                    )
                    .size(12.0)
                    .color(p.muted),
                );
            }

            ui.add_space(18.0);
            heading(ui, p, "About");
            ui.label(
                RichText::new(format!("Ferrodock {}", env!("CARGO_PKG_VERSION"))).color(p.text),
            );
            ui.horizontal(|ui| {
                ui.hyperlink_to("Website", "https://manimovassagh.github.io/Ferrodock/");
                ui.label(RichText::new("·").color(p.muted));
                ui.hyperlink_to("GitHub", "https://github.com/manimovassagh/Ferrodock");
                ui.label(RichText::new("·").color(p.muted));
                ui.hyperlink_to(
                    "Report a problem",
                    "https://github.com/manimovassagh/Ferrodock/issues/new/choose",
                );
            });

            ui.add_space(18.0);
            ui.horizontal(|ui| {
                if ui.button("Reset to defaults").clicked() {
                    *settings = Settings::default();
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    close = ui.button("Done").clicked();
                });
            });
        });
    if *settings != before {
        settings.apply(ui.ctx());
    }
    !(close || modal.should_close())
}

fn heading(ui: &mut egui::Ui, p: &Palette, text: &str) {
    ui.label(RichText::new(text.to_uppercase()).size(11.0).color(p.muted));
    ui.add_space(6.0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn light_is_the_default() {
        let s = Settings::default();
        assert_eq!(s.theme, Theme::Light);
        assert_eq!(s.density, Density::Comfortable);
        assert_eq!(s.scale, 1.0);
    }

    #[test]
    fn missing_fields_fall_back_to_defaults() {
        // Settings saved by an older version may lack newer fields.
        let s: Settings = serde_json::from_str(r#"{"theme":"Dark"}"#).unwrap();
        assert_eq!(s.theme, Theme::Dark);
        assert_eq!(s.scale, 1.0);
    }
}
