//! "New volume" and "New network" dialogs, and the volume ones that fill
//! the new volume from a backup or another volume.

use std::path::{Path, PathBuf};

use eframe::egui::{self, CornerRadius, Frame, Margin, RichText, Stroke};

use crate::app::components::widgets;
use crate::docker::Change;
use crate::theme::Palette;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Volume,
    Network,
}

/// Where a new volume's files come from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// A backup made with Back up (a `.tar.gz`), or any tar: the path is
    /// the form's `archive`.
    Backup,
    /// A copy of this volume.
    Volume(String),
}

/// The dialog's fields while it's open.
pub struct CreateForm {
    kind: Kind,
    /// Only for volumes: none makes an empty one.
    source: Option<Source>,
    /// The backup to restore, typed or chosen.
    archive: String,
    name: String,
    driver: String,
    subnet: String,
    internal: bool,
}

pub enum Outcome {
    Create(Change),
    Cancel,
}

impl CreateForm {
    pub fn new(kind: Kind) -> Self {
        Self {
            kind,
            source: None,
            archive: String::new(),
            name: String::new(),
            driver: String::new(),
            subnet: String::new(),
            internal: false,
        }
    }

    /// A new volume from a backup, typed or chosen in the dialog (the
    /// system's file picker isn't there on every Linux desktop).
    pub fn restore() -> Self {
        Self {
            source: Some(Source::Backup),
            ..Self::new(Kind::Volume)
        }
    }

    /// Points the form at another backup. The name follows the file's
    /// until it's changed by hand.
    fn set_archive(&mut self, archive: String) {
        if self.name.trim() == volume_name_of(Path::new(self.archive.trim())) {
            self.name = volume_name_of(Path::new(archive.trim()));
        }
        self.archive = archive;
    }

    /// A new volume with a copy of `volume`'s files.
    pub fn clone_of(volume: &str) -> Self {
        Self {
            name: format!("{volume}-copy"),
            source: Some(Source::Volume(volume.to_owned())),
            ..Self::new(Kind::Volume)
        }
    }

    /// The change to make, or what's wrong with the form.
    fn change(&self) -> Result<Change, &'static str> {
        let name = self.name.trim();
        if name.is_empty() {
            return Err("Give it a name");
        }
        if !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "_.-".contains(c))
        {
            return Err("Names can only use letters, digits, _ . and -");
        }
        let subnet = self.subnet.trim();
        if !subnet.is_empty() && !looks_like_cidr(subnet) {
            return Err("Write the subnet like 172.30.0.0/16");
        }
        let (name, driver) = (name.to_owned(), self.driver.trim().to_owned());
        Ok(match self.kind {
            Kind::Volume => match self.source.clone() {
                None => Change::CreateVolume { name, driver },
                Some(Source::Backup) => {
                    let archive = PathBuf::from(self.archive.trim());
                    if !archive.is_file() {
                        return Err("Type or choose the backup file");
                    }
                    Change::RestoreVolume { name, archive }
                }
                Some(Source::Volume(from)) => Change::CloneVolume { from, to: name },
            },
            Kind::Network => Change::CreateNetwork {
                name,
                driver,
                subnet: subnet.to_owned(),
                internal: self.internal,
            },
        })
    }
}

/// Asks for a volume backup with the system's file picker.
fn pick_backup() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Restore a volume from a backup")
        .add_filter("Volume backup", &["gz", "tgz", "tar"])
        .pick_file()
}

/// The volume a backup file was made from: `web-data (1).tar.gz` →
/// `web-data`.
fn volume_name_of(archive: &Path) -> String {
    let file = archive
        .file_name()
        .map(|f| f.to_string_lossy().into_owned())
        .unwrap_or_default();
    let stem = [".tar.gz", ".tgz", ".tar"]
        .iter()
        .find_map(|ext| file.strip_suffix(ext))
        .unwrap_or(&file);
    // The number Back up adds when the name was taken.
    let stem = match stem.rsplit_once(" (") {
        Some((name, n))
            if n.strip_suffix(')')
                .is_some_and(|n| n.parse::<u32>().is_ok()) =>
        {
            name
        }
        _ => stem,
    };
    stem.to_owned()
}

/// `a.b.c.d/n` (IPv4) or anything with a `:` and a prefix length (IPv6).
fn looks_like_cidr(s: &str) -> bool {
    let Some((address, prefix)) = s.split_once('/') else {
        return false;
    };
    let prefix_ok = prefix.parse::<u8>().is_ok_and(|n| n <= 128);
    let v4 = address.split('.').count() == 4 && address.split('.').all(|o| o.parse::<u8>().is_ok());
    prefix_ok && (v4 || address.contains(':'))
}

pub fn show(ui: &mut egui::Ui, p: &Palette, form: &mut CreateForm) -> Option<Outcome> {
    let mut outcome = None;
    let (title, driver_hint, action) = match (&form.kind, &form.source) {
        (Kind::Volume, None) => ("New volume", "local", "Create"),
        (Kind::Volume, Some(Source::Backup)) => ("Restore a volume", "", "Restore"),
        (Kind::Volume, Some(Source::Volume(_))) => ("Clone a volume", "", "Clone"),
        (Kind::Network, _) => ("New network", "bridge", "Create"),
    };
    let from = match &form.source {
        Some(Source::Volume(volume)) => Some(volume.clone()),
        _ => None,
    };
    let modal = egui::Modal::new(egui::Id::unique("ferrodock-create"))
        .frame(
            Frame::new()
                .fill(p.sidebar)
                .stroke(Stroke::new(1.0, p.border))
                .corner_radius(CornerRadius::same(12))
                .inner_margin(Margin::same(20)),
        )
        .show(ui.ctx(), |ui| {
            ui.set_width(400.0);
            ui.label(
                RichText::new(title)
                    .font(egui::FontId::new(18.0, crate::theme::semibold()))
                    .color(p.text),
            );
            ui.add_space(12.0);
            egui::Grid::new("create-form")
                .num_columns(2)
                .spacing([16.0, 10.0])
                .show(ui, |ui| {
                    ui.label(RichText::new("Name").color(p.muted));
                    let name = ui.add(
                        egui::TextEdit::singleline(&mut form.name).desired_width(f32::INFINITY),
                    );
                    if form.name.is_empty() {
                        name.request_focus();
                    }
                    ui.end_row();
                    if form.source == Some(Source::Backup) {
                        ui.label(RichText::new("Backup").color(p.muted));
                        ui.horizontal(|ui| {
                            let choose = ui.button("Choose…");
                            let mut archive = form.archive.clone();
                            ui.add(
                                egui::TextEdit::singleline(&mut archive)
                                    .hint_text("~/Downloads/web-data.tar.gz")
                                    .desired_width(ui.available_width()),
                            );
                            if choose.clicked()
                                && let Some(picked) = pick_backup()
                            {
                                archive = picked.display().to_string();
                            }
                            if archive != form.archive {
                                form.set_archive(archive);
                            }
                        });
                        ui.end_row();
                    } else if let Some(from) = &from {
                        ui.label(RichText::new("From").color(p.muted));
                        ui.add(egui::Label::new(RichText::new(from).color(p.text)).truncate());
                        ui.end_row();
                    } else {
                        ui.label(RichText::new("Driver").color(p.muted));
                        ui.add(
                            egui::TextEdit::singleline(&mut form.driver)
                                .hint_text(driver_hint)
                                .desired_width(f32::INFINITY),
                        );
                        ui.end_row();
                    }
                    if form.kind == Kind::Network {
                        ui.label(RichText::new("Subnet").color(p.muted));
                        ui.add(
                            egui::TextEdit::singleline(&mut form.subnet)
                                .hint_text("Docker picks one")
                                .desired_width(f32::INFINITY),
                        );
                        ui.end_row();
                        ui.label("");
                        ui.checkbox(&mut form.internal, "Internal (no internet access)");
                        ui.end_row();
                    }
                });
            ui.add_space(10.0);
            let change = form.change();
            if let (Err(problem), false) = (&change, form.name.is_empty()) {
                ui.label(RichText::new(*problem).color(p.danger));
            }
            ui.add_space(8.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let create = ui
                    .add_enabled_ui(change.is_ok(), |ui| widgets::primary_button(ui, action))
                    .inner;
                let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                if (create.clicked() || enter)
                    && let Ok(change) = change
                {
                    outcome = Some(Outcome::Create(change));
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
    fn validates_the_form() {
        let mut form = CreateForm::new(Kind::Network);
        assert!(form.change().is_err());
        form.name = "backend".into();
        assert!(matches!(form.change(), Ok(Change::CreateNetwork { .. })));
        form.subnet = "10.1.0.0".into();
        assert!(form.change().is_err());
        form.subnet = "10.1.0.0/16".into();
        assert!(form.change().is_ok());
        form.name = "has space".into();
        assert!(form.change().is_err());
    }

    #[test]
    fn restores_and_clones_into_a_named_volume() {
        let dir = std::env::temp_dir().join(format!("ferrodock-restore-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let backup = dir.join("web-data (2).tar.gz");
        std::fs::write(&backup, b"").unwrap();
        let mut form = CreateForm::restore();
        assert!(form.change().is_err());
        form.set_archive(backup.display().to_string());
        assert_eq!(form.name, "web-data");
        assert_eq!(
            form.change(),
            Ok(Change::RestoreVolume {
                name: "web-data".into(),
                archive: backup.clone(),
            })
        );
        // A name typed by hand stays when another backup is picked.
        form.name = "mine".into();
        form.set_archive(dir.join("db.tar").display().to_string());
        assert_eq!(form.name, "mine");
        assert_eq!(form.change(), Err("Type or choose the backup file"));
        assert_eq!(volume_name_of(Path::new("db.tar")), "db");
        assert_eq!(volume_name_of(Path::new("odd (x).tgz")), "odd (x)");
        assert_eq!(
            CreateForm::clone_of("db").change(),
            Ok(Change::CloneVolume {
                from: "db".into(),
                to: "db-copy".into(),
            })
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn checks_subnets() {
        assert!(looks_like_cidr("172.30.0.0/16"));
        assert!(looks_like_cidr("fd00::/64"));
        assert!(!looks_like_cidr("172.30.0/16"));
        assert!(!looks_like_cidr("172.30.0.0"));
    }
}
