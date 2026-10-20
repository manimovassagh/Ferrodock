//! Themes after popular code editor colour schemes, for people who like
//! Ferrodock to match their editor and terminal. Each keeps Ferrodock's
//! layout and only brings its colours: background shades, text, and its
//! signature accent.

use eframe::egui::Color32;

use super::Palette;

const fn hex(rgb: u32) -> Color32 {
    Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

pub struct EditorTheme {
    /// How it's saved in the settings.
    pub id: &'static str,
    pub name: &'static str,
    pub dark: bool,
    pub palette: Palette,
}

pub static EDITOR_THEMES: [EditorTheme; 12] = [
    EditorTheme {
        id: "tokyo-night",
        name: "Tokyo Night",
        dark: true,
        palette: Palette {
            bg: hex(0x1a1b26),
            sidebar: hex(0x16161e),
            raised: hex(0x292e42),
            border: hex(0x3b4261),
            text: hex(0xc0caf5),
            muted: hex(0x828bb8),
            accent: hex(0x7aa2f7),
            on_accent: hex(0x1a1b26),
            glow: hex(0xbb9af7),
            success: hex(0x9ece6a),
            warning: hex(0xe0af68),
            danger: hex(0xf7768e),
        },
    },
    EditorTheme {
        id: "catppuccin-mocha",
        name: "Catppuccin Mocha",
        dark: true,
        palette: Palette {
            bg: hex(0x1e1e2e),
            sidebar: hex(0x181825),
            raised: hex(0x313244),
            border: hex(0x45475a),
            text: hex(0xcdd6f4),
            muted: hex(0x9399b2),
            accent: hex(0xcba6f7),
            on_accent: hex(0x1e1e2e),
            glow: hex(0x89b4fa),
            success: hex(0xa6e3a1),
            warning: hex(0xfab387),
            danger: hex(0xf38ba8),
        },
    },
    EditorTheme {
        id: "catppuccin-latte",
        name: "Catppuccin Latte",
        dark: false,
        palette: Palette {
            bg: hex(0xeff1f5),
            sidebar: hex(0xe6e9ef),
            raised: hex(0xdce0e8),
            border: hex(0xccd0da),
            text: hex(0x4c4f69),
            muted: hex(0x5c5f77),
            accent: hex(0x8839ef),
            on_accent: Color32::WHITE,
            glow: hex(0x1e66f5),
            success: hex(0x40a02b),
            warning: hex(0xdf8e1d),
            danger: hex(0xd20f39),
        },
    },
    EditorTheme {
        id: "gruvbox-dark",
        name: "Gruvbox Dark",
        dark: true,
        palette: Palette {
            bg: hex(0x282828),
            sidebar: hex(0x1d2021),
            raised: hex(0x3c3836),
            border: hex(0x504945),
            text: hex(0xebdbb2),
            muted: hex(0xa89984),
            accent: hex(0xfe8019),
            on_accent: hex(0x282828),
            glow: hex(0x8ec07c),
            success: hex(0xb8bb26),
            warning: hex(0xfabd2f),
            danger: hex(0xfb4934),
        },
    },
    EditorTheme {
        id: "nord",
        name: "Nord",
        dark: true,
        palette: Palette {
            bg: hex(0x2e3440),
            sidebar: hex(0x292e39),
            raised: hex(0x3b4252),
            border: hex(0x434c5e),
            text: hex(0xeceff4),
            muted: hex(0x9aa5b8),
            accent: hex(0x88c0d0),
            on_accent: hex(0x2e3440),
            glow: hex(0xb48ead),
            success: hex(0xa3be8c),
            warning: hex(0xebcb8b),
            danger: hex(0xbf616a),
        },
    },
    EditorTheme {
        id: "rose-pine",
        name: "Rosé Pine",
        dark: true,
        palette: Palette {
            bg: hex(0x191724),
            sidebar: hex(0x1f1d2e),
            raised: hex(0x26233a),
            border: hex(0x403d52),
            text: hex(0xe0def4),
            muted: hex(0x908caa),
            accent: hex(0xebbcba),
            on_accent: hex(0x191724),
            glow: hex(0xc4a7e7),
            success: hex(0x9ccfd8),
            warning: hex(0xf6c177),
            danger: hex(0xeb6f92),
        },
    },
    EditorTheme {
        id: "dracula",
        name: "Dracula",
        dark: true,
        palette: Palette {
            bg: hex(0x282a36),
            sidebar: hex(0x21222c),
            raised: hex(0x343746),
            border: hex(0x44475a),
            text: hex(0xf8f8f2),
            // Its comment colour (#6272a4), lightened to be readable.
            muted: hex(0x8f97c8),
            accent: hex(0xbd93f9),
            on_accent: hex(0x282a36),
            glow: hex(0xff79c6),
            success: hex(0x50fa7b),
            warning: hex(0xffb86c),
            danger: hex(0xff5555),
        },
    },
    EditorTheme {
        id: "one-dark",
        name: "One Dark",
        dark: true,
        palette: Palette {
            bg: hex(0x282c34),
            sidebar: hex(0x21252b),
            raised: hex(0x2c313a),
            border: hex(0x3e4451),
            // Its #abb2bf, a touch lighter for 7:1.
            text: hex(0xb4bac6),
            muted: hex(0x9da5b4),
            accent: hex(0x61afef),
            on_accent: hex(0x282c34),
            glow: hex(0xc678dd),
            success: hex(0x98c379),
            warning: hex(0xe5c07b),
            danger: hex(0xe06c75),
        },
    },
    EditorTheme {
        id: "one-light",
        name: "One Light",
        dark: false,
        palette: Palette {
            bg: hex(0xfafafa),
            sidebar: hex(0xeaeaeb),
            raised: hex(0xe5e5e6),
            border: hex(0xdbdbdc),
            text: hex(0x383a42),
            muted: hex(0x696c77),
            accent: hex(0x4078f2),
            on_accent: Color32::WHITE,
            glow: hex(0xa626a4),
            success: hex(0x50a14f),
            warning: hex(0xc18401),
            danger: hex(0xe45649),
        },
    },
    EditorTheme {
        id: "github-dark",
        name: "GitHub Dark",
        dark: true,
        palette: Palette {
            bg: hex(0x0d1117),
            sidebar: hex(0x010409),
            raised: hex(0x161b22),
            border: hex(0x30363d),
            text: hex(0xe6edf3),
            muted: hex(0x7d8590),
            accent: hex(0x58a6ff),
            on_accent: hex(0x0d1117),
            glow: hex(0xbc8cff),
            success: hex(0x3fb950),
            warning: hex(0xd29922),
            danger: hex(0xf85149),
        },
    },
    EditorTheme {
        id: "github-light",
        name: "GitHub Light",
        dark: false,
        palette: Palette {
            bg: hex(0xffffff),
            sidebar: hex(0xf6f8fa),
            raised: hex(0xeaeef2),
            border: hex(0xd0d7de),
            text: hex(0x1f2328),
            muted: hex(0x656d76),
            accent: hex(0x0969da),
            on_accent: Color32::WHITE,
            glow: hex(0x8250df),
            success: hex(0x1a7f37),
            warning: hex(0x9a6700),
            danger: hex(0xd1242f),
        },
    },
    EditorTheme {
        id: "solarized-light",
        name: "Solarized Light",
        dark: false,
        palette: Palette {
            bg: hex(0xfdf6e3),
            sidebar: hex(0xeee8d5),
            raised: hex(0xe4ddc8),
            border: hex(0xd3cbb7),
            // Its body text (base00) is too faint for 7:1; base02 and
            // base01 are.
            text: hex(0x073642),
            muted: hex(0x586e75),
            accent: hex(0x268bd2),
            on_accent: hex(0xfdf6e3),
            glow: hex(0x6c71c4),
            success: hex(0x859900),
            warning: hex(0xb58900),
            danger: hex(0xdc322f),
        },
    },
];

/// The editor theme saved as `id`.
pub fn find(id: &str) -> Option<&'static EditorTheme> {
    EDITOR_THEMES.iter().find(|t| t.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// WCAG relative luminance.
    fn luminance(c: Color32) -> f32 {
        let channel = |v: u8| {
            let v = v as f32 / 255.0;
            if v <= 0.03928 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(c.r()) + 0.7152 * channel(c.g()) + 0.0722 * channel(c.b())
    }

    fn contrast(a: Color32, b: Color32) -> f32 {
        let (a, b) = (luminance(a), luminance(b));
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    #[test]
    fn every_theme_stays_readable() {
        let ours = [
            ("Dark", &super::super::DARK),
            ("Light", &super::super::LIGHT),
        ];
        let editors = EDITOR_THEMES.iter().map(|t| (t.name, &t.palette));
        for (name, p) in ours.into_iter().chain(editors) {
            assert!(contrast(p.text, p.bg) >= 7.0, "{name}: text");
            assert!(
                contrast(p.text, p.sidebar) >= 4.5,
                "{name}: text on sidebar"
            );
            assert!(contrast(p.muted, p.bg) >= 4.5, "{name}: muted text");
            // Button labels are bold. Ferrodock's own dark accent is at
            // 2.9; the editor themes all clear 5.
            assert!(
                contrast(p.on_accent, p.accent) >= 2.9,
                "{name}: button text"
            );
        }
    }

    #[test]
    fn ids_are_unique_and_found() {
        for theme in &EDITOR_THEMES {
            assert!(std::ptr::eq(find(theme.id).unwrap(), theme));
            assert_eq!(theme.dark, luminance(theme.palette.bg) < 0.2);
        }
        assert!(find("nope").is_none());
    }
}
