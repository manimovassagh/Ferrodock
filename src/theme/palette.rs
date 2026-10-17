//! IronStack's colours, in a dark and a light variant.

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
    bg: Color32::from_rgb(0x16, 0x18, 0x1d),
    sidebar: Color32::from_rgb(0x1c, 0x1f, 0x26),
    raised: Color32::from_rgb(0x26, 0x2a, 0x34),
    border: Color32::from_rgb(0x2e, 0x33, 0x40),
    text: Color32::from_rgb(0xe6, 0xe8, 0xee),
    muted: Color32::from_rgb(0x8b, 0x93, 0xa7),
    accent: Color32::from_rgb(0x5b, 0x8c, 0xff),
    success: Color32::from_rgb(0x3f, 0xb9, 0x50),
    warning: Color32::from_rgb(0xd2, 0x99, 0x22),
    danger: Color32::from_rgb(0xf8, 0x51, 0x49),
};

pub const LIGHT: Palette = Palette {
    bg: Color32::from_rgb(0xff, 0xff, 0xff),
    sidebar: Color32::from_rgb(0xf4, 0xf5, 0xf7),
    raised: Color32::from_rgb(0xec, 0xee, 0xf2),
    border: Color32::from_rgb(0xe1, 0xe4, 0xea),
    text: Color32::from_rgb(0x1f, 0x23, 0x28),
    muted: Color32::from_rgb(0x65, 0x6d, 0x7e),
    accent: Color32::from_rgb(0x2f, 0x6b, 0xff),
    success: Color32::from_rgb(0x1a, 0x7f, 0x37),
    warning: Color32::from_rgb(0x9a, 0x67, 0x00),
    danger: Color32::from_rgb(0xcf, 0x22, 0x2e),
};

/// The palette matching the theme currently in use.
pub fn palette(ui: &egui::Ui) -> &'static Palette {
    if ui.visuals().dark_mode {
        &DARK
    } else {
        &LIGHT
    }
}
