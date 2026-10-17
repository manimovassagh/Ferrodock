//! The parts of the main area every page shares: the header and the
//! "Docker isn't running" screen.

use eframe::egui::{self, Align, Layout, RichText};

use super::components::widgets;
use crate::theme::Palette;

/// The search box, so `/` can focus it.
pub fn search_id() -> egui::Id {
    egui::Id::unique("ferrodock-search")
}

/// Page title, summary line, the page's own toolbar, search box and
/// refresh button.
/// Returns true when refresh was clicked.
pub fn header(
    ui: &mut egui::Ui,
    p: &Palette,
    title: &str,
    summary: &str,
    search_hint: &str,
    search: &mut String,
    toolbar: impl FnOnce(&mut egui::Ui),
) -> bool {
    let mut refresh = false;
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label(RichText::new(title).size(22.0).strong().color(p.text));
            ui.label(RichText::new(summary).color(p.muted));
        });
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            refresh = widgets::icon_button(ui, "⟳", "Refresh (F5)").clicked();
            ui.add(
                egui::TextEdit::singleline(search)
                    .id(search_id())
                    .hint_text(search_hint)
                    .desired_width(240.0)
                    .margin(egui::vec2(8.0, 5.0)),
            )
            .on_hover_text(format!(
                "Press / to search this page, {}K to search everything",
                super::keys::COMMAND
            ));
            toolbar(ui);
        });
    });
    refresh
}

/// Shown instead of the page while the engine is unreachable.
/// Returns true when "Retry now" was clicked.
pub fn disconnected(ui: &mut egui::Ui, p: &Palette, reason: &str) -> bool {
    let mut retry = false;
    ui.add_space(60.0);
    ui.vertical_centered(|ui| {
        ui.label(
            RichText::new("Docker isn't running")
                .size(18.0)
                .strong()
                .color(p.text),
        );
        ui.add_space(6.0);
        ui.label(
            RichText::new(
                "Start Docker Desktop (or another Docker Engine). \
                 Ferrodock connects automatically once it's up.",
            )
            .color(p.muted),
        );
        ui.add_space(4.0);
        ui.label(RichText::new(reason).size(12.0).color(p.muted));
        ui.add_space(14.0);
        retry = ui.button("Retry now").clicked();
    });
    retry
}
