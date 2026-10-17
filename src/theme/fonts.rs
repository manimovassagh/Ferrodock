//! Fonts: the platform's UI font where available.

use eframe::egui::{
    self, FontData, FontFamily,
    epaint::text::{FontInsert, FontPriority, InsertFontFamily},
};

/// Uses the platform UI font when it's available, so Ferrodock looks at
/// home next to other native apps. egui's bundled fonts stay as fallback
/// (they also provide the icon glyphs).
pub fn load_system_font(ctx: &egui::Context) {
    #[cfg(windows)]
    const CANDIDATES: &[&str] = &[r"C:\Windows\Fonts\segoeui.ttf"];
    #[cfg(not(windows))]
    const CANDIDATES: &[&str] = &[];

    if let Some(data) = CANDIDATES.iter().find_map(|path| std::fs::read(path).ok()) {
        ctx.add_font(FontInsert::new(
            "system-ui",
            FontData::from_owned(data),
            vec![InsertFontFamily {
                family: FontFamily::Proportional,
                priority: FontPriority::Highest,
            }],
        ));
    }
}
