//! "New volume" and "New network" dialogs.

use eframe::egui::{self, CornerRadius, Frame, Margin, RichText, Stroke};

use crate::app::components::widgets;
use crate::docker::Change;
use crate::theme::Palette;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Volume,
    Network,
}

/// The dialog's fields while it's open.
pub struct CreateForm {
    kind: Kind,
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
            name: String::new(),
            driver: String::new(),
            subnet: String::new(),
            internal: false,
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
            Kind::Volume => Change::CreateVolume { name, driver },
            Kind::Network => Change::CreateNetwork {
                name,
                driver,
                subnet: subnet.to_owned(),
                internal: self.internal,
            },
        })
    }
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
    let (title, driver_hint) = match form.kind {
        Kind::Volume => ("New volume", "local"),
        Kind::Network => ("New network", "bridge"),
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
                    ui.label(RichText::new("Driver").color(p.muted));
                    ui.add(
                        egui::TextEdit::singleline(&mut form.driver)
                            .hint_text(driver_hint)
                            .desired_width(f32::INFINITY),
                    );
                    ui.end_row();
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
                    .add_enabled_ui(change.is_ok(), |ui| widgets::primary_button(ui, "Create"))
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
    fn checks_subnets() {
        assert!(looks_like_cidr("172.30.0.0/16"));
        assert!(looks_like_cidr("fd00::/64"));
        assert!(!looks_like_cidr("172.30.0/16"));
        assert!(!looks_like_cidr("172.30.0.0"));
    }
}
