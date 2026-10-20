//! Ferrodock's look: colour palettes, widget styling and fonts.
//!
//! A dark and a light palette are installed; egui picks one based on the
//! theme setting (or the operating system's). Ferrodock's own palettes are
//! the default; an editor theme takes the place of the one it matches.

mod editors;
mod fonts;
mod palette;
mod visuals;

use eframe::egui::{self, Theme, Visuals};

pub use editors::{EDITOR_THEMES, EditorTheme, find as editor_theme};
pub use fonts::semibold;
pub use palette::{DARK, LIGHT, Palette, palette};

/// Applies Ferrodock's fonts, colours and spacing to the egui context.
pub fn install(ctx: &egui::Context) {
    fonts::install(ctx);
    use_palettes(ctx, &DARK, &LIGHT);
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

/// Uses `dark` in dark mode and `light` in light mode.
pub fn use_palettes(ctx: &egui::Context, dark: &'static Palette, light: &'static Palette) {
    ctx.set_visuals_of(Theme::Dark, visuals::visuals(Visuals::dark(), dark));
    ctx.set_visuals_of(Theme::Light, visuals::visuals(Visuals::light(), light));
    ctx.data_mut(|d| d.insert_temp(palette::active_id(), palette::Active { dark, light }));
}
