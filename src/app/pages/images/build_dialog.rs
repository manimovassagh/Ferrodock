//! The Build dialog: build an image from a folder with a Dockerfile.

use std::path::{Path, PathBuf};

use eframe::egui::{self, CornerRadius, Frame, Margin, RichText, Stroke};

use crate::app::components::widgets;
use crate::docker::BuildSpec;
use crate::theme::Palette;

#[derive(Default)]
pub(super) struct BuildForm {
    /// The build context.
    pub folder: String,
    /// Relative to the folder.
    pub dockerfile: String,
    pub tag: String,
    /// `KEY=value`, one per line.
    pub args: String,
    pub target: String,
    pub no_cache: bool,
}

impl BuildForm {
    /// A form for building `folder`, tagged after it.
    pub fn for_folder(folder: &Path) -> Self {
        let mut form = Self {
            dockerfile: "Dockerfile".into(),
            ..Self::default()
        };
        form.set_folder(folder);
        form
    }

    fn set_folder(&mut self, folder: &Path) {
        self.folder = folder.display().to_string();
        self.tag = format!("{}:latest", default_name(folder));
    }

    /// The build to start, or what's wrong with the form.
    pub fn spec(&self, job: u64) -> Result<BuildSpec, String> {
        let folder = PathBuf::from(self.folder.trim());
        if !folder.is_dir() {
            return Err("Pick the folder to build".into());
        }
        let dockerfile = self.dockerfile.trim();
        if !folder.join(dockerfile).is_file() {
            return Err(format!("There's no {dockerfile} in that folder"));
        }
        let tag = self.tag.trim();
        let name = tag.rsplit_once(':').map_or(tag, |(name, _)| name);
        if !super::tag_dialog::valid(tag) || name.chars().any(|c| c.is_ascii_uppercase()) {
            return Err("Name it with lowercase letters, digits, . _ - and / , then :tag".into());
        }
        let mut args = Vec::new();
        for line in self.args.lines().map(str::trim).filter(|l| !l.is_empty()) {
            match line.split_once('=') {
                Some((key, value)) if !key.trim().is_empty() => {
                    args.push((key.trim().to_owned(), value.to_owned()));
                }
                _ => return Err(format!("Write build arguments as KEY=value, not {line}")),
            }
        }
        Ok(BuildSpec {
            job,
            context: folder,
            dockerfile: dockerfile.to_owned(),
            tag: tag.to_owned(),
            args,
            target: self.target.trim().to_owned(),
            no_cache: self.no_cache,
        })
    }
}

/// An image name made from a folder's: `My App` → `my-app`.
fn default_name(folder: &Path) -> String {
    let name: String = folder
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || "._-".contains(c) {
                c
            } else {
                '-'
            }
        })
        .collect();
    let name = name.trim_matches(|c: char| !c.is_ascii_alphanumeric());
    if name.is_empty() {
        "image".into()
    } else {
        name.to_owned()
    }
}

/// Asks for a folder to build with the system's folder picker.
pub(super) fn pick_folder() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Build an image from a folder")
        .pick_folder()
}

/// Some(Some(spec)) to build (its job number still to set), Some(None) to
/// cancel, None while open.
pub(super) fn show(
    ui: &mut egui::Ui,
    p: &Palette,
    form: &mut BuildForm,
) -> Option<Option<BuildSpec>> {
    let mut outcome = None;
    let modal = egui::Modal::new(egui::Id::unique("ferrodock-build"))
        .frame(
            Frame::new()
                .fill(p.sidebar)
                .stroke(Stroke::new(1.0, p.border))
                .corner_radius(CornerRadius::same(12))
                .inner_margin(Margin::same(20)),
        )
        .show(ui.ctx(), |ui| {
            ui.set_width(480.0);
            ui.label(
                RichText::new("Build an image")
                    .font(egui::FontId::new(18.0, crate::theme::semibold()))
                    .color(p.text),
            );
            ui.label(RichText::new("From a folder with a Dockerfile.").color(p.muted));
            ui.add_space(12.0);
            egui::Grid::new("build-form")
                .num_columns(2)
                .spacing([16.0, 10.0])
                .show(ui, |ui| {
                    ui.label(RichText::new("Folder").color(p.muted));
                    ui.horizontal(|ui| {
                        let browse = ui.button("Choose…");
                        ui.add(
                            egui::TextEdit::singleline(&mut form.folder)
                                .desired_width(ui.available_width()),
                        );
                        if browse.clicked()
                            && let Some(folder) = pick_folder()
                        {
                            form.set_folder(&folder);
                        }
                    });
                    ui.end_row();
                    ui.label(RichText::new("Dockerfile").color(p.muted));
                    ui.add(
                        egui::TextEdit::singleline(&mut form.dockerfile)
                            .desired_width(f32::INFINITY),
                    );
                    ui.end_row();
                    ui.label(RichText::new("Name").color(p.muted));
                    ui.add(
                        egui::TextEdit::singleline(&mut form.tag)
                            .hint_text("my-app:latest")
                            .desired_width(f32::INFINITY),
                    );
                    ui.end_row();
                    ui.label(RichText::new("Build arguments").color(p.muted));
                    ui.add(
                        egui::TextEdit::multiline(&mut form.args)
                            .hint_text("KEY=value, one per line")
                            .desired_rows(2)
                            .desired_width(f32::INFINITY),
                    );
                    ui.end_row();
                    ui.label(RichText::new("Target stage").color(p.muted));
                    ui.add(
                        egui::TextEdit::singleline(&mut form.target)
                            .hint_text("The last one")
                            .desired_width(f32::INFINITY),
                    );
                    ui.end_row();
                    ui.label("");
                    ui.checkbox(&mut form.no_cache, "Don't use the build cache");
                    ui.end_row();
                });
            ui.add_space(10.0);
            let spec = form.spec(0);
            if let (Err(problem), false) = (&spec, form.folder.trim().is_empty()) {
                ui.label(RichText::new(problem).color(p.danger));
            }
            ui.add_space(8.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let build = ui
                    .add_enabled_ui(spec.is_ok(), |ui| widgets::primary_button(ui, "Build"))
                    .inner;
                if build.clicked()
                    && let Ok(spec) = spec
                {
                    outcome = Some(Some(spec));
                }
                if ui.button("Cancel").clicked() {
                    outcome = Some(None);
                }
            });
        });
    if outcome.is_none() && modal.should_close() {
        outcome = Some(None);
    }
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_the_image_after_the_folder() {
        assert_eq!(default_name(Path::new("/src/My App")), "my-app");
        assert_eq!(default_name(Path::new("/src/api_v2")), "api_v2");
        assert_eq!(default_name(Path::new("/")), "image");
    }

    #[test]
    fn checks_the_form() {
        let dir = std::env::temp_dir().join(format!("ferrodock-build-form-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut form = BuildForm::for_folder(&dir);
        assert!(form.spec(1).unwrap_err().contains("no Dockerfile"));
        std::fs::write(dir.join("Dockerfile"), "FROM scratch\n").unwrap();
        form.args = "VERSION=1.2\n\nEMPTY=\n".into();
        let spec = form.spec(1).unwrap();
        assert_eq!(spec.tag, format!("{}:latest", default_name(&dir)));
        assert_eq!(
            spec.args,
            [
                ("VERSION".into(), "1.2".into()),
                ("EMPTY".into(), String::new())
            ]
        );
        form.args = "nonsense".into();
        assert!(form.spec(1).is_err());
        form.args.clear();
        form.tag = "MyApp".into();
        assert!(form.spec(1).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
