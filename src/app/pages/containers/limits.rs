//! The Limits dialog: change a container's CPU and memory limits without
//! recreating it.

use eframe::egui::{self, CornerRadius, Frame, Margin, RichText, Stroke};

use crate::app::components::widgets;
use crate::app::page::{Intent, PageCtx};
use crate::docker::{
    Change, ContainerInfo, RunSettings, cpus_text, format_size, memory_text, parse_cpus,
    parse_memory,
};
use crate::theme::Palette;

/// "1.5 CPUs · 512 MB", or "None" without limits.
pub fn describe(nano_cpus: i64, memory: i64) -> String {
    let mut parts = Vec::new();
    if nano_cpus > 0 {
        let cpus = cpus_text(nano_cpus);
        let unit = if cpus == "1" { "CPU" } else { "CPUs" };
        parts.push(format!("{cpus} {unit}"));
    }
    if memory > 0 {
        parts.push(format!("{} memory", format_size(memory)));
    }
    if parts.is_empty() {
        "None".into()
    } else {
        parts.join(" · ")
    }
}

/// The dialog's fields while it's open.
pub struct LimitsForm {
    id: String,
    name: String,
    cpus: String,
    memory: String,
    /// The limits when the dialog opened; the Engine can't remove one.
    had_cpus: bool,
    had_memory: bool,
}

impl LimitsForm {
    pub fn new(c: &ContainerInfo, run: &RunSettings) -> Self {
        Self::for_container(&c.id, &c.name, run)
    }

    fn for_container(id: &str, name: &str, run: &RunSettings) -> Self {
        Self {
            id: id.to_owned(),
            name: name.to_owned(),
            cpus: cpus_text(run.nano_cpus),
            memory: memory_text(run.memory),
            had_cpus: run.nano_cpus > 0,
            had_memory: run.memory > 0,
        }
    }

    /// The change to make, or what's wrong with the form.
    fn change(&self) -> Result<Change, &'static str> {
        const REMOVE: &str = "A limit can't be removed in place. Use Edit… to recreate the \
                              container without it";
        let nano_cpus = match self.cpus.trim() {
            "" if self.had_cpus => return Err(REMOVE),
            "" => 0,
            text => parse_cpus(text).ok_or("Write CPUs as a number, like 1.5")?,
        };
        let memory = match self.memory.trim() {
            "" if self.had_memory => return Err(REMOVE),
            "" => 0,
            text => parse_memory(text).ok_or("Write memory like 512m or 2g")?,
        };
        Ok(Change::SetLimits {
            id: self.id.clone(),
            name: self.name.clone(),
            nano_cpus,
            memory,
        })
    }
}

/// Draws the dialog in `slot` while it's open, and applies the change
/// once the user confirms.
pub fn show_open(ui: &mut egui::Ui, ctx: &mut PageCtx, slot: &mut Option<LimitsForm>) {
    let Some(form) = slot else {
        return;
    };
    match show(ui, ctx.palette, form) {
        Some(Some(change)) => ctx.request(Intent::Change(change)),
        Some(None) => {}
        None => return,
    }
    *slot = None;
}

/// Some(Some(change)) to apply, Some(None) to cancel, None while open.
fn show(ui: &mut egui::Ui, p: &Palette, form: &mut LimitsForm) -> Option<Option<Change>> {
    let mut outcome = None;
    let modal = egui::Modal::new(egui::Id::unique("ferrodock-limits"))
        .frame(
            Frame::new()
                .fill(p.sidebar)
                .stroke(Stroke::new(1.0, p.border))
                .corner_radius(CornerRadius::same(12))
                .inner_margin(Margin::same(20)),
        )
        .show(ui.ctx(), |ui| {
            ui.set_width(380.0);
            ui.label(
                RichText::new(format!("Limits for {}", form.name))
                    .font(egui::FontId::new(18.0, crate::theme::semibold()))
                    .color(p.text),
            );
            ui.label(RichText::new("Takes effect right away, without a restart.").color(p.muted));
            ui.add_space(12.0);
            egui::Grid::new("limits-form")
                .num_columns(2)
                .spacing([16.0, 10.0])
                .show(ui, |ui| {
                    ui.label(RichText::new("CPUs").color(p.muted));
                    ui.add(
                        egui::TextEdit::singleline(&mut form.cpus)
                            .hint_text("No limit, e.g. 1.5")
                            .desired_width(f32::INFINITY),
                    );
                    ui.end_row();
                    ui.label(RichText::new("Memory").color(p.muted));
                    ui.add(
                        egui::TextEdit::singleline(&mut form.memory)
                            .hint_text("No limit, e.g. 512m")
                            .desired_width(f32::INFINITY),
                    );
                    ui.end_row();
                });
            ui.add_space(10.0);
            let change = form.change();
            if let Err(problem) = &change {
                ui.label(RichText::new(*problem).color(p.danger));
            }
            ui.add_space(8.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let apply = ui
                    .add_enabled_ui(change.is_ok(), |ui| widgets::primary_button(ui, "Apply"))
                    .inner;
                if apply.clicked()
                    && let Ok(change) = change
                {
                    outcome = Some(Some(change));
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

    fn form(nano_cpus: i64, memory: i64) -> LimitsForm {
        LimitsForm::for_container(
            "abc",
            "web",
            &RunSettings {
                nano_cpus,
                memory,
                ..Default::default()
            },
        )
    }

    #[test]
    fn describes_limits() {
        assert_eq!(describe(0, 0), "None");
        assert_eq!(describe(1_000_000_000, 0), "1 CPU");
        assert!(describe(1_500_000_000, 512 << 20).starts_with("1.5 CPUs · "));
    }

    #[test]
    fn validates_the_form() {
        let mut f = form(0, 0);
        assert!(f.change().is_ok());
        f.memory = "1g".into();
        assert!(matches!(
            f.change(),
            Ok(Change::SetLimits { memory, .. }) if memory == 1 << 30
        ));
        f.cpus = "many".into();
        assert!(f.change().is_err());
        // An existing limit can be changed, not cleared.
        let mut f = form(2_000_000_000, 0);
        assert_eq!(f.cpus, "2");
        f.cpus.clear();
        assert!(f.change().is_err());
    }
}
