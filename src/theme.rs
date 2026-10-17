//! IronStack's look: colour palette, spacing and fonts.
//!
//! Both a dark and a light palette are installed; egui picks one based on
//! the operating system's theme.

use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontFamily, Stroke, Theme, Visuals,
    epaint::text::{FontInsert, FontPriority, InsertFontFamily},
};

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

pub fn install(ctx: &egui::Context) {
    load_system_font(ctx);
    ctx.set_visuals_of(Theme::Dark, visuals(Visuals::dark(), &DARK));
    ctx.set_visuals_of(Theme::Light, visuals(Visuals::light(), &LIGHT));
    ctx.all_styles_mut(|style| {
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.button_padding = egui::vec2(10.0, 4.0);
        style.spacing.interact_size.y = 26.0;
    });
}

fn visuals(mut v: Visuals, p: &Palette) -> Visuals {
    let radius = CornerRadius::same(6);

    v.panel_fill = p.bg;
    v.window_fill = p.sidebar;
    v.window_stroke = Stroke::new(1.0, p.border);
    v.window_corner_radius = CornerRadius::same(8);
    v.extreme_bg_color = p.sidebar;
    v.faint_bg_color = p.sidebar;
    v.hyperlink_color = p.accent;
    v.error_fg_color = p.danger;
    v.warn_fg_color = p.warning;
    v.selection.bg_fill = p.accent.gamma_multiply(0.35);
    v.selection.stroke = Stroke::new(1.0, p.text);

    let w = &mut v.widgets;
    w.noninteractive.bg_stroke = Stroke::new(1.0, p.border);
    w.noninteractive.fg_stroke = Stroke::new(1.0, p.text);
    w.noninteractive.corner_radius = radius;

    w.inactive.weak_bg_fill = p.raised;
    w.inactive.bg_fill = p.raised;
    w.inactive.bg_stroke = Stroke::NONE;
    w.inactive.fg_stroke = Stroke::new(1.0, p.text);
    w.inactive.corner_radius = radius;

    w.hovered.weak_bg_fill = p.border;
    w.hovered.bg_fill = p.border;
    w.hovered.bg_stroke = Stroke::NONE;
    w.hovered.fg_stroke = Stroke::new(1.0, p.text);
    w.hovered.corner_radius = radius;
    w.hovered.expansion = 0.0;

    w.active.weak_bg_fill = p.accent.gamma_multiply(0.5);
    w.active.bg_fill = p.accent.gamma_multiply(0.5);
    w.active.bg_stroke = Stroke::NONE;
    w.active.fg_stroke = Stroke::new(1.0, p.text);
    w.active.corner_radius = radius;
    w.active.expansion = 0.0;

    w.open = w.active;
    v
}

/// Uses the platform UI font when it's available, so IronStack looks at
/// home next to other native apps. egui's bundled fonts stay as fallback
/// (they also provide the icon glyphs).
fn load_system_font(ctx: &egui::Context) {
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
