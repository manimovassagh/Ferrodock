//! The Create dialog of a Linux machine: name, version, a folder to share
//! and whether to keep it.

use std::path::{Path, PathBuf};

use eframe::egui::{self, CornerRadius, Frame, Margin, RichText, Stroke};

use crate::app::components::widgets;
use crate::app::store::Store;
use crate::docker::{Distro, MachineSpec, MachineUser};
use crate::theme::Palette;

pub struct MachineForm {
    pub distro: &'static Distro,
    tag: String,
    name: String,
    /// A folder of this computer to share, typed or chosen; empty for none.
    folder: String,
    throwaway: bool,
}

pub enum Outcome {
    Create(Box<MachineSpec>),
    Cancel,
}

impl MachineForm {
    /// Named after the distribution, numbered past the names taken.
    pub fn new(distro: &'static Distro, store: &Store) -> Self {
        let taken = |name: &str| name_taken(store, name);
        let name = std::iter::once(distro.id.to_owned())
            .chain((2..).map(|n| format!("{}-{n}", distro.id)))
            .find(|name| !taken(name))
            .unwrap_or_default();
        Self {
            distro,
            tag: distro.tags[0].to_owned(),
            name,
            folder: String::new(),
            throwaway: false,
        }
    }

    /// The machine to create, or what's wrong with the form.
    pub fn checked(&self, store: &Store) -> Result<MachineSpec, String> {
        let name = self.name.trim();
        if !valid_name(name) {
            return Err(
                "Name it with letters, digits and -, starting with a letter or digit".into(),
            );
        }
        if name_taken(store, name) {
            return Err(format!("There's already a container named {name}"));
        }
        let folder = match self.folder.trim() {
            "" => None,
            typed => {
                let path = expand_home(typed);
                if !path.is_dir() {
                    return Err("There's no folder there to share".into());
                }
                Some(path)
            }
        };
        Ok(MachineSpec {
            distro: self.distro,
            tag: self.tag.clone(),
            name: name.to_owned(),
            user: MachineUser::current(folder.as_deref()),
            folder: folder.map(|f| f.display().to_string()),
            throwaway: self.throwaway,
        })
    }
}

/// A container name that's also a valid hostname.
fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 63
        && name.starts_with(|c: char| c.is_ascii_alphanumeric())
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

/// Whether a container, or a machine being created, has this name.
fn name_taken(store: &Store, name: &str) -> bool {
    store.containers.iter().any(|c| c.name == name)
        || store.pending.contains_key(&format!("machine:{name}"))
}

/// `~/code` → `/home/me/code`.
fn expand_home(path: &str) -> PathBuf {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"));
    match (path.strip_prefix("~"), home) {
        (Some(rest), Some(home)) if rest.is_empty() || rest.starts_with(['/', '\\']) => {
            Path::new(&home).join(rest.trim_start_matches(['/', '\\']))
        }
        _ => PathBuf::from(path),
    }
}

/// Asks for a folder to share with the system's folder picker.
fn pick_folder() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Share a folder with the machine")
        .pick_folder()
}

/// Draws the dialog. Returns what the user chose, once they choose.
pub fn show(
    ui: &mut egui::Ui,
    p: &Palette,
    store: &Store,
    form: &mut MachineForm,
) -> Option<Outcome> {
    let distro = form.distro;
    let mut outcome = None;
    let modal = egui::Modal::new(egui::Id::unique("ferrodock-machine"))
        .frame(
            Frame::new()
                .fill(p.sidebar)
                .stroke(Stroke::new(1.0, p.border))
                .corner_radius(CornerRadius::same(12))
                .inner_margin(Margin::same(20)),
        )
        .show(ui.ctx(), |ui| {
            ui.set_width(500.0);
            ui.horizontal(|ui| {
                super::badge(ui, distro, 40.0);
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new(format!("New {} machine", distro.name))
                            .font(egui::FontId::new(18.0, crate::theme::semibold()))
                            .color(p.text),
                    );
                    ui.label(RichText::new(distro.summary).size(12.5).color(p.muted));
                });
            });
            ui.add_space(14.0);
            let checked = form.checked(store);
            egui::Grid::new("machine-form")
                .num_columns(2)
                .spacing([16.0, 10.0])
                .show(ui, |ui| {
                    let label = ui.label(RichText::new("Name").color(p.muted));
                    ui.add(egui::TextEdit::singleline(&mut form.name).desired_width(f32::INFINITY))
                        .labelled_by(label.id);
                    ui.end_row();

                    ui.label(RichText::new("Version").color(p.muted));
                    egui::ComboBox::from_id_salt("machine-tag")
                        .selected_text(&form.tag)
                        .show_ui(ui, |ui| {
                            for tag in distro.tags {
                                ui.selectable_value(&mut form.tag, (*tag).to_owned(), *tag);
                            }
                        });
                    ui.end_row();

                    let label = ui.label(RichText::new("Shared folder").color(p.muted));
                    ui.horizontal(|ui| {
                        let browse = ui.button("Choose…");
                        ui.add(
                            egui::TextEdit::singleline(&mut form.folder)
                                .hint_text("None")
                                .desired_width(ui.available_width()),
                        )
                        .labelled_by(label.id);
                        if browse.clicked()
                            && let Some(folder) = pick_folder()
                        {
                            form.folder = folder.display().to_string();
                        }
                    });
                    ui.end_row();
                    ui.label("");
                    ui.checkbox(&mut form.throwaway, "Throwaway: delete it when it stops");
                    ui.end_row();
                });
            if let Ok(spec) = &checked
                && let Some(mount) = spec.mount_point()
            {
                ui.add_space(8.0);
                ui.label(
                    RichText::new(format!(
                        "The folder shows up as {mount} in the machine, where its shell \
                         starts. Changes show on both sides."
                    ))
                    .size(12.0)
                    .color(p.muted),
                );
            }
            ui.add_space(12.0);
            ui.horizontal_wrapped(|ui| match &checked {
                Err(problem) => {
                    ui.label(RichText::new(problem).size(12.0).color(p.danger));
                }
                Ok(spec) => {
                    ui.label(
                        RichText::new(format!(
                            "Gets {} if it isn't here, then adds you as {} with sudo, \
                                 git and a few tools.",
                            spec.image(),
                            spec.user.name
                        ))
                        .size(12.0)
                        .color(p.muted),
                    );
                }
            });
            ui.add_space(10.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let create = ui
                    .add_enabled_ui(checked.is_ok(), |ui| widgets::primary_button(ui, "Create"))
                    .inner;
                if create.clicked()
                    && let Ok(spec) = checked
                {
                    outcome = Some(Outcome::Create(Box::new(spec)));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checks_names() {
        assert!(valid_name("ubuntu-2"));
        assert!(valid_name("Box1"));
        assert!(!valid_name("-box"));
        assert!(!valid_name("my box"));
        assert!(!valid_name("a_b"));
        assert!(!valid_name(&"a".repeat(64)));
    }

    #[test]
    fn numbers_past_taken_names() {
        let mut store = Store::new();
        let ubuntu = crate::docker::distro("ubuntu").unwrap();
        assert_eq!(MachineForm::new(ubuntu, &store).name, "ubuntu");
        store.containers.push(crate::docker::ContainerInfo {
            name: "ubuntu".into(),
            ..Default::default()
        });
        store
            .pending
            .insert("machine:ubuntu-2".into(), crate::docker::Action::Run);
        let form = MachineForm::new(ubuntu, &store);
        assert_eq!(form.name, "ubuntu-3");
        assert!(form.checked(&store).is_ok());
    }

    #[test]
    fn checks_the_folder() {
        let store = Store::new();
        let mut form = MachineForm::new(crate::docker::distro("debian").unwrap(), &store);
        form.folder = "/no/such/folder".into();
        assert!(form.checked(&store).is_err());
        form.folder = std::env::temp_dir().display().to_string();
        let spec = form.checked(&store).unwrap();
        assert_eq!(
            spec.folder,
            Some(std::env::temp_dir().display().to_string())
        );
        assert_eq!(expand_home("/x"), PathBuf::from("/x"));
        if let Some(home) = std::env::var_os("HOME") {
            assert_eq!(expand_home("~/code"), Path::new(&home).join("code"));
            assert_eq!(expand_home("~"), PathBuf::from(&home));
        }
    }
}
