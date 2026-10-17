//! Ferrodock's colours, in a dark and a light variant.

use eframe::egui::{self, Color32};

pub struct Palette {
    /// Main content background.
    pub bg: Color32,
    /// Sidebar and header background.
    pub sidebar: Color32,
    /// Buttons, hovered rows, input fields.
    pub raised: Color32,
    pub border: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub accent: Color32,
    pub success: Color32,
    pub warning: Color32,
    pub danger: Color32,
}

pub const DARK: Palette = Palette {
    bg: Color32::from_rgb(0x1c, 0x1c, 0x1f),
    sidebar: Color32::from_rgb(0x24, 0x24, 0x28),
    raised: Color32::from_rgb(0x2e, 0x2e, 0x33),
    border: Color32::from_rgb(0x3a, 0x3a, 0x40),
    text: Color32::from_rgb(0xf2, 0xf2, 0xf5),
    muted: Color32::from_rgb(0x9a, 0x9a, 0xa3),
    accent: Color32::from_rgb(0xf0, 0x71, 0x4f),
    success: Color32::from_rgb(0x32, 0xd1, 0x5c),
    warning: Color32::from_rgb(0xff, 0xa8, 0x26),
    danger: Color32::from_rgb(0xff, 0x5a, 0x50),
};

pub const LIGHT: Palette = Palette {
    bg: Color32::from_rgb(0xff, 0xff, 0xff),
    sidebar: Color32::from_rgb(0xf5, 0xf5, 0xf7),
    raised: Color32::from_rgb(0xee, 0xee, 0xf1),
    border: Color32::from_rgb(0xe4, 0xe4, 0xe9),
    text: Color32::from_rgb(0x1d, 0x1d, 0x1f),
    muted: Color32::from_rgb(0x6e, 0x6e, 0x76),
    accent: Color32::from_rgb(0xc8, 0x48, 0x2a),
    success: Color32::from_rgb(0x1f, 0x8f, 0x3e),
    warning: Color32::from_rgb(0xb2, 0x5e, 0x00),
    danger: Color32::from_rgb(0xd7, 0x26, 0x2c),
};

/// The palette matching the theme currently in use.
pub fn palette(ui: &egui::Ui) -> &'static Palette {
    if ui.visuals().dark_mode {
        &DARK
    } else {
        &LIGHT
    }
}
