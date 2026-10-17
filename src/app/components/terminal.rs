//! A terminal screen: draws a `vt100` screen as a grid of cells and turns
//! key presses into the bytes a shell expects.

use eframe::egui::{self, Color32, CornerRadius, Event, Key, Rect, Sense, Stroke, pos2, vec2};

use super::ansi::ansi_color;
use crate::theme::Palette;

const FONT_SIZE: f32 = 13.0;
const PADDING: f32 = 8.0;

/// What a frame of the terminal produced.
pub struct Output {
    /// Bytes to send to the shell.
    pub input: Vec<u8>,
    /// The size (columns, rows) that fits the space it was given.
    pub size: (u16, u16),
}

/// Draws `screen` filling the space left, and reads keys while focused.
/// It takes focus when clicked, or right away when `focus` is set.
pub fn show(ui: &mut egui::Ui, p: &Palette, screen: &vt100::Screen, focus: bool) -> Output {
    let font = egui::FontId::monospace(FONT_SIZE);
    let cell = vec2(
        ui.fonts_mut(|f| f.glyph_width(&font, 'M')).max(1.0),
        ui.fonts_mut(|f| f.row_height(&font)).max(1.0),
    );
    let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::click());
    let inner = rect.shrink(PADDING);
    let size = (
        ((inner.width() / cell.x).floor() as u16).max(20),
        ((inner.height() / cell.y).floor() as u16).max(4),
    );
    if response.clicked() || focus {
        response.request_focus();
    }
    let focused = response.has_focus();
    let background = terminal_background(p);
    let painter = ui.painter_at(rect);
    painter.rect(
        rect,
        CornerRadius::same(8),
        background,
        Stroke::new(1.0, if focused { p.accent } else { p.border }),
        egui::StrokeKind::Inside,
    );

    let (rows, cols) = screen.size();
    for row in 0..rows {
        let y = inner.top() + row as f32 * cell.y;
        let mut job = egui::text::LayoutJob::default();
        for col in 0..cols {
            let Some(c) = screen.cell(row, col) else {
                continue;
            };
            if c.is_wide_continuation() {
                continue;
            }
            let (mut fg, mut bg) = (
                color(p, c.fgcolor()).unwrap_or(p.text),
                color(p, c.bgcolor()),
            );
            if c.inverse() {
                (fg, bg) = (bg.unwrap_or(background), Some(fg));
            }
            if c.dim() {
                fg = fg.gamma_multiply(0.6);
            }
            let width = if c.is_wide() { 2.0 } else { 1.0 };
            if let Some(bg) = bg {
                painter.rect_filled(
                    Rect::from_min_size(
                        pos2(inner.left() + col as f32 * cell.x, y),
                        vec2(cell.x * width, cell.y),
                    ),
                    0.0,
                    bg,
                );
            }
            let text = if c.has_contents() { c.contents() } else { " " };
            job.append(
                text,
                0.0,
                egui::TextFormat {
                    font_id: font.clone(),
                    color: fg,
                    underline: if c.underline() {
                        Stroke::new(1.0, fg)
                    } else {
                        Stroke::NONE
                    },
                    italics: c.italic(),
                    ..Default::default()
                },
            );
        }
        let galley = painter.layout_job(job);
        painter.galley(pos2(inner.left(), y), galley, p.text);
    }

    if !screen.hide_cursor() {
        let (row, col) = screen.cursor_position();
        let at = Rect::from_min_size(
            pos2(
                inner.left() + col as f32 * cell.x,
                inner.top() + row as f32 * cell.y,
            ),
            cell,
        );
        if focused {
            painter.rect_filled(at, 1.0, p.accent.gamma_multiply(0.55));
        } else {
            painter.rect_stroke(at, 1.0, Stroke::new(1.0, p.muted), egui::StrokeKind::Inside);
        }
    }

    let mut input = Vec::new();
    if focused {
        // Keep Tab, arrows and Escape for the shell, not focus moves.
        ui.memory_mut(|m| {
            m.set_focus_lock_filter(
                response.id,
                egui::EventFilter {
                    tab: true,
                    horizontal_arrows: true,
                    vertical_arrows: true,
                    escape: true,
                },
            );
        });
        let events = ui.input(|i| i.events.clone());
        let keys = Keys {
            app_cursor: screen.application_cursor(),
            bracketed_paste: screen.bracketed_paste(),
            mac: cfg!(target_os = "macos"),
        };
        for event in &events {
            if matches!(event, Event::Copy) && keys.mac {
                ui.ctx().copy_text(screen.contents());
                continue;
            }
            if let Some(bytes) = keys.encode(event) {
                input.extend(bytes);
            }
        }
    }
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::TextInput, true, "Terminal"));
    Output { input, size }
}

/// A shade darker (or lighter) than the page, so the terminal stands out.
fn terminal_background(p: &Palette) -> Color32 {
    if p.bg.r() < 128 {
        Color32::from_rgb(0x12, 0x12, 0x14)
    } else {
        Color32::from_rgb(0xfb, 0xfb, 0xfc)
    }
}

/// A terminal colour; `None` for the default.
fn color(p: &Palette, c: vt100::Color) -> Option<Color32> {
    match c {
        vt100::Color::Default => None,
        vt100::Color::Idx(i) if i < 16 => Some(ansi_color(p, i)),
        vt100::Color::Idx(i) if i < 232 => {
            // The 6×6×6 colour cube.
            let i = i - 16;
            let level = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
            Some(Color32::from_rgb(
                level(i / 36),
                level(i / 6 % 6),
                level(i % 6),
            ))
        }
        vt100::Color::Idx(i) => {
            let gray = 8 + (i - 232) * 10;
            Some(Color32::from_gray(gray))
        }
        vt100::Color::Rgb(r, g, b) => Some(Color32::from_rgb(r, g, b)),
    }
}

/// How keys are encoded, which depends on modes the shell set.
struct Keys {
    /// Arrows send `ESC O A` instead of `ESC [ A` (vim, less, …).
    app_cursor: bool,
    /// Pasted text is wrapped so the shell knows it wasn't typed.
    bracketed_paste: bool,
    /// On macOS ⌘C copies; elsewhere Ctrl+C (which egui reports as Copy)
    /// interrupts.
    mac: bool,
}

impl Keys {
    fn encode(&self, event: &Event) -> Option<Vec<u8>> {
        match event {
            Event::Text(text) => Some(text.as_bytes().to_vec()),
            Event::Paste(text) => {
                let text = text.replace("\r\n", "\r").replace('\n', "\r");
                Some(if self.bracketed_paste {
                    format!("\x1b[200~{text}\x1b[201~").into_bytes()
                } else {
                    text.into_bytes()
                })
            }
            Event::Copy if !self.mac => Some(vec![0x03]),
            Event::Cut if !self.mac => Some(vec![0x18]),
            Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } => self.key(*key, *modifiers),
            _ => None,
        }
    }

    fn key(&self, key: Key, modifiers: egui::Modifiers) -> Option<Vec<u8>> {
        if modifiers.ctrl && !modifiers.alt {
            let name = key.name();
            if name.len() == 1 && name.as_bytes()[0].is_ascii_uppercase() {
                return Some(vec![name.as_bytes()[0] - b'A' + 1]);
            }
        }
        let arrow = |c: char| {
            let prefix = if self.app_cursor { "\x1bO" } else { "\x1b[" };
            Some(format!("{prefix}{c}").into_bytes())
        };
        let bytes: &[u8] = match key {
            Key::Enter => b"\r",
            Key::Backspace => b"\x7f",
            Key::Tab if modifiers.shift => b"\x1b[Z",
            Key::Tab => b"\t",
            Key::Escape => b"\x1b",
            Key::ArrowUp => return arrow('A'),
            Key::ArrowDown => return arrow('B'),
            Key::ArrowRight => return arrow('C'),
            Key::ArrowLeft => return arrow('D'),
            Key::Home => b"\x1b[H",
            Key::End => b"\x1b[F",
            Key::PageUp => b"\x1b[5~",
            Key::PageDown => b"\x1b[6~",
            Key::Delete => b"\x1b[3~",
            Key::Insert => b"\x1b[2~",
            _ => return None,
        };
        Some(bytes.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(key: Key, modifiers: egui::Modifiers) -> Event {
        Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }
    }

    #[test]
    fn encodes_keys() {
        let keys = Keys {
            app_cursor: false,
            bracketed_paste: false,
            mac: false,
        };
        let none = egui::Modifiers::NONE;
        assert_eq!(keys.encode(&Event::Text("ls".into())), Some(b"ls".to_vec()));
        assert_eq!(keys.encode(&press(Key::Enter, none)), Some(b"\r".to_vec()));
        assert_eq!(
            keys.encode(&press(Key::ArrowUp, none)),
            Some(b"\x1b[A".to_vec())
        );
        assert_eq!(
            keys.encode(&press(Key::D, egui::Modifiers::CTRL)),
            Some(vec![4])
        );
        assert_eq!(keys.encode(&Event::Copy), Some(vec![3]));
        assert_eq!(
            keys.encode(&press(Key::A, none)),
            None,
            "letters come as Text"
        );
        let vim = Keys {
            app_cursor: true,
            bracketed_paste: true,
            mac: true,
        };
        assert_eq!(
            vim.encode(&press(Key::ArrowUp, none)),
            Some(b"\x1bOA".to_vec())
        );
        assert_eq!(
            vim.encode(&Event::Paste("a\nb".into())),
            Some(b"\x1b[200~a\rb\x1b[201~".to_vec())
        );
        assert_eq!(vim.encode(&Event::Copy), None);
    }

    #[test]
    fn maps_colours() {
        let p = crate::theme::LIGHT;
        assert_eq!(color(&p, vt100::Color::Default), None);
        assert_eq!(
            color(&p, vt100::Color::Idx(196)),
            Some(Color32::from_rgb(255, 0, 0))
        );
        assert_eq!(
            color(&p, vt100::Color::Rgb(1, 2, 3)),
            Some(Color32::from_rgb(1, 2, 3))
        );
    }
}
