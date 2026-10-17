//! Window-wide keyboard shortcuts: what each key does, and the list the
//! "?" overlay shows.

use eframe::egui::{self, Event, Key, Modifiers};

use super::components::shortcuts::Section;

/// What a key press asks the app to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shortcut {
    Palette,
    Help,
    Settings,
    /// Switch to the page with this index.
    Page(usize),
    /// Move the selection in the list up (-1) or down (+1).
    Step(i32),
    CloseDetails,
    FocusSearch,
    Refresh,
    /// A key the open page handles itself, e.g. `S` for start/stop.
    PageKey(Key),
}

/// "⌘" on macOS, "Ctrl+" elsewhere, for showing shortcuts.
pub const COMMAND: &str = if cfg!(target_os = "macos") {
    "⌘"
} else {
    "Ctrl+"
};

/// Keys a page can react to (see [`super::page::Page::shortcut`]).
const PAGE_KEYS: [Key; 6] = [Key::S, Key::R, Key::P, Key::L, Key::Delete, Key::Backspace];

/// Reads this frame's shortcuts. Single-key shortcuts are skipped while a
/// text field has focus, so typing never triggers them.
pub fn read(ctx: &egui::Context) -> Vec<Shortcut> {
    let typing = ctx.egui_wants_keyboard_input();
    ctx.input_mut(|i| {
        let mut found = Vec::new();
        if i.consume_key(Modifiers::COMMAND, Key::K) {
            found.push(Shortcut::Palette);
        }
        if i.consume_key(Modifiers::COMMAND, Key::Comma) {
            found.push(Shortcut::Settings);
        }
        let digits = [
            Key::Num1,
            Key::Num2,
            Key::Num3,
            Key::Num4,
            Key::Num5,
            Key::Num6,
        ];
        for (index, key) in digits.into_iter().enumerate() {
            if i.consume_key(Modifiers::COMMAND, key) {
                found.push(Shortcut::Page(index));
            }
        }
        if i.consume_key(Modifiers::NONE, Key::F5) || i.consume_key(Modifiers::COMMAND, Key::R) {
            found.push(Shortcut::Refresh);
        }
        if typing {
            return found;
        }
        let typed = |text: &str| {
            i.events
                .iter()
                .any(|e| matches!(e, Event::Text(t) if t == text))
        };
        if typed("?") {
            found.push(Shortcut::Help);
        }
        if typed("/") {
            found.push(Shortcut::FocusSearch);
        }
        if i.consume_key(Modifiers::NONE, Key::ArrowDown) || i.consume_key(Modifiers::NONE, Key::J)
        {
            found.push(Shortcut::Step(1));
        }
        if i.consume_key(Modifiers::NONE, Key::ArrowUp) || i.consume_key(Modifiers::NONE, Key::K) {
            found.push(Shortcut::Step(-1));
        }
        if i.consume_key(Modifiers::NONE, Key::Escape) {
            found.push(Shortcut::CloseDetails);
        }
        for key in PAGE_KEYS {
            if i.consume_key(Modifiers::NONE, key) {
                found.push(Shortcut::PageKey(key));
            }
        }
        found
    })
}

/// Everything the "?" overlay lists.
pub fn help(page_titles: &[&'static str]) -> Vec<Section> {
    let pages = page_titles
        .iter()
        .enumerate()
        .map(|(i, title)| (format!("{COMMAND}{}", i + 1), *title))
        .collect();
    vec![
        (
            "Anywhere",
            vec![
                (format!("{COMMAND}K"), "Command palette"),
                (format!("{COMMAND},"), "Settings"),
                ("/".into(), "Search this page"),
                ("F5".into(), "Refresh"),
                ("?".into(), "This list"),
            ],
        ),
        (
            "Lists",
            vec![
                ("↑ ↓  or  J K".into(), "Select previous / next"),
                ("Esc".into(), "Close the details pane"),
            ],
        ),
        (
            "Selected container",
            vec![
                ("S".into(), "Start or stop"),
                ("R".into(), "Restart"),
                ("P".into(), "Pause or resume"),
                ("L".into(), "Show logs"),
                ("⌫ / Delete".into(), "Remove (asks first)"),
            ],
        ),
        ("Pages", pages),
    ]
}
