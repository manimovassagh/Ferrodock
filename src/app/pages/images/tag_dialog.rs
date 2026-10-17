//! The Tag dialog: give an image another name, e.g. to push it.

use eframe::egui::{self, CornerRadius, Frame, Margin, RichText, Stroke};

use crate::app::components::widgets;
use crate::theme::Palette;

pub(super) struct TagForm {
    pub image_id: String,
    /// The image's current name, shown in the title.
    pub current: String,
    pub target: String,
}

/// Whether `target` looks like an image name: `[registry/]repo[:tag]`,
/// lowercase repository, no spaces.
fn valid(target: &str) -> bool {
    let target = target.trim();
    let repo = match target.rsplit_once(':') {
        Some((repo, tag)) if !tag.contains('/') => {
            if tag.is_empty() || tag.len() > 128 {
                return false;
            }
            repo
        }
        _ => target,
    };
    !repo.is_empty()
        && !target.contains(char::is_whitespace)
        && repo.split('/').skip(1).all(|part| {
            !part.is_empty()
                && part
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "._-".contains(c))
        })
}

/// Some(Some(target)) to tag, Some(None) to cancel, None while open.
pub(super) fn show(ui: &mut egui::Ui, p: &Palette, form: &mut TagForm) -> Option<Option<String>> {
    let mut outcome = None;
    let ok = valid(&form.target);
    let modal = egui::Modal::new(egui::Id::unique("ferrodock-tag"))
        .frame(
            Frame::new()
                .fill(p.sidebar)
                .stroke(Stroke::new(1.0, p.border))
                .corner_radius(CornerRadius::same(12))
                .inner_margin(Margin::same(20)),
        )
        .show(ui.ctx(), |ui| {
            ui.set_width(420.0);
            ui.label(
                RichText::new(format!("Tag {}", form.current))
                    .font(egui::FontId::new(18.0, crate::theme::semibold()))
                    .color(p.text),
            );
            ui.label(
                RichText::new("To push to a registry, start the name with it.").color(p.muted),
            );
            ui.add_space(10.0);
            let field = ui.add(
                egui::TextEdit::singleline(&mut form.target)
                    .hint_text("ghcr.io/me/app:1.0")
                    .desired_width(f32::INFINITY),
            );
            field.request_focus();
            if !form.target.trim().is_empty() && !ok {
                ui.label(
                    RichText::new("Use lowercase letters, digits, . _ - and / , then :tag")
                        .color(p.danger),
                );
            }
            ui.add_space(12.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let tag = ui
                    .add_enabled_ui(ok, |ui| widgets::primary_button(ui, "Tag"))
                    .inner;
                if ok && (tag.clicked() || ui.input(|i| i.key_pressed(egui::Key::Enter))) {
                    outcome = Some(Some(form.target.trim().to_owned()));
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
    use super::valid;

    #[test]
    fn checks_names() {
        assert!(valid("myapp"));
        assert!(valid("ghcr.io/me/app:1.0"));
        assert!(valid("localhost:5000/app"));
        assert!(!valid("My App"));
        assert!(!valid("me/App:1"));
        assert!(!valid("app:"));
        assert!(!valid(""));
    }
}
