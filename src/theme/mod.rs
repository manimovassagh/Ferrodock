//! Ferrodock's look: colour palettes, widget styling and fonts.
//!
//! Both a dark and a light palette are installed; egui picks one based on
//! the operating system's theme.

mod fonts;
mod palette;
mod visuals;

use eframe::egui::{self, Theme, Visuals};

pub use fonts::semibold;
pub use palette::{DARK, LIGHT, Palette, palette};

/// Applies Ferrodock's fonts, colours and spacing to the egui context.
pub fn install(ctx: &egui::Context) {
    fonts::install(ctx);
    ctx.set_visuals_of(Theme::Dark, visuals::visuals(Visuals::dark(), &DARK));
    ctx.set_visuals_of(Theme::Light, visuals::visuals(Visuals::light(), &LIGHT));
    ctx.all_styles_mut(|style| {
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.button_padding = egui::vec2(12.0, 5.0);
        style.spacing.interact_size.y = 28.0;
        style
            .text_styles
            .insert(egui::TextStyle::Body, egui::FontId::proportional(13.5));
        style
            .text_styles
            .insert(egui::TextStyle::Button, egui::FontId::proportional(13.5));
    });
}
