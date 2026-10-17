//! The parts of the main area every page shares: the header and the
//! "Docker isn't running" screen.

use eframe::egui::{self, Align, Layout, RichText};

use super::components::icons::{self, Icon};
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
            ui.label(
                RichText::new(title)
                    .font(egui::FontId::new(24.0, crate::theme::semibold()))
                    .color(p.text),
            );
            ui.label(RichText::new(summary).color(p.muted));
        });
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            refresh = widgets::icon_button(ui, Icon::Refresh, "Refresh (F5)").clicked();
            search_box(ui, p, search_hint, search);
            toolbar(ui);
        });
    });
    refresh
}

/// A rounded search field with a magnifier icon.
fn search_box(ui: &mut egui::Ui, p: &Palette, hint: &str, search: &mut String) {
    egui::Frame::new()
        .fill(p.raised)
        .corner_radius(egui::CornerRadius::same(8))
        .inner_margin(egui::Margin::symmetric(9, 0))
        .show(ui, |ui| {
            ui.set_width(240.0);
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                icons::show(ui, Icon::Search, 14.0, p.muted);
                ui.add(
                    egui::TextEdit::singleline(search)
                        .id(search_id())
                        .hint_text(hint)
                        .frame(egui::Frame::NONE)
                        .desired_width(220.0)
                        .margin(egui::vec2(2.0, 5.0)),
                )
                .on_hover_text(format!(
                    "Press / to search this page, {}K to search everything",
                    super::keys::COMMAND
                ));
            });
        });
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
