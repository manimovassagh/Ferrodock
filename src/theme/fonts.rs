//! Fonts: Inter for the interface (bundled, so Ferrodock looks the same on
//! every platform) and egui's Hack for monospace. Inter is licensed under
//! the SIL Open Font License; see `assets/fonts/Inter-LICENSE.txt`.

use std::sync::Arc;

use eframe::egui::{self, FontData, FontDefinitions, FontFamily};

const INTER: &[u8] = include_bytes!("../../assets/fonts/Inter-Regular.ttf");
const INTER_SEMIBOLD: &[u8] = include_bytes!("../../assets/fonts/Inter-SemiBold.ttf");

/// The family for titles and emphasis.
pub fn semibold() -> FontFamily {
    FontFamily::Name("semibold".into())
}

pub fn install(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    fonts
        .font_data
        .insert("Inter".into(), Arc::new(FontData::from_static(INTER)));
    fonts.font_data.insert(
        "Inter-SemiBold".into(),
        Arc::new(FontData::from_static(INTER_SEMIBOLD)),
    );

    // Inter first; egui's own fonts stay behind it as fallbacks.
    let proportional = fonts.families.entry(FontFamily::Proportional).or_default();
    proportional.insert(0, "Inter".into());
    let fallbacks = proportional.clone();

    let mut bold = vec!["Inter-SemiBold".to_owned()];
    bold.extend(fallbacks.into_iter().filter(|f| f != "Inter"));
    fonts.families.insert(semibold(), bold);

    // Monospace falls back to Inter for symbols Hack lacks.
    fonts
        .families
        .entry(FontFamily::Monospace)
        .or_default()
        .push("Inter".into());

    ctx.set_fonts(fonts);
}
