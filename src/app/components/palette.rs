//! The ⌘K / Ctrl+K command palette: a search box over everything the app
//! can jump to or do. The app supplies the entries; this only draws them,
//! ranks them and reports which one was picked.

use eframe::egui::{self, CornerRadius, FontId, Frame, Key, Margin, Sense, Stroke, vec2};

use crate::theme::Palette;

/// Most results shown at once; typing more narrows them down.
const MAX_RESULTS: usize = 50;
const ROW_HEIGHT: f32 = 34.0;

pub struct Entry {
    pub label: String,
    /// Dimmed text on the right, e.g. "Container · running".
    pub detail: String,
}

pub enum Outcome {
    /// Index of the picked entry.
    Chosen(usize),
    Closed,
}

#[derive(Default)]
pub struct CommandPalette {
    query: String,
    highlighted: usize,
}

impl CommandPalette {
    /// Draws the palette over the window. `entries` come in their default
    /// order (recently used first), which also breaks ties when ranking.
    pub fn show(&mut self, ui: &mut egui::Ui, p: &Palette, entries: &[Entry]) -> Option<Outcome> {
        let results = rank(&self.query, entries);
        let mut outcome = None;

        // Read the navigation keys before the text box can act on them.
        let (down, up, enter) = ui.input_mut(|i| {
            (
                i.consume_key(egui::Modifiers::NONE, Key::ArrowDown)
                    || i.consume_key(egui::Modifiers::NONE, Key::Tab),
                i.consume_key(egui::Modifiers::NONE, Key::ArrowUp)
                    || i.consume_key(egui::Modifiers::SHIFT, Key::Tab),
                i.consume_key(egui::Modifiers::NONE, Key::Enter),
            )
        });
        let last = results.len().saturating_sub(1);
        if down {
            self.highlighted = (self.highlighted + 1).min(last);
        }
        if up {
            self.highlighted = self.highlighted.saturating_sub(1);
        }
        self.highlighted = self.highlighted.min(last);
        if enter && let Some(&index) = results.get(self.highlighted) {
            outcome = Some(Outcome::Chosen(index));
        }

        let modal = egui::Modal::new(egui::Id::unique("ferrodock-palette"))
            .frame(
                Frame::new()
                    .fill(p.sidebar)
                    .stroke(Stroke::new(1.0, p.border))
                    .corner_radius(CornerRadius::same(12))
                    .inner_margin(Margin::same(10)),
            )
            .show(ui.ctx(), |ui| {
                ui.set_width(560.0);
                let search = ui.add(
                    egui::TextEdit::singleline(&mut self.query)
                        .hint_text("Search containers, images, actions…")
                        .font(FontId::proportional(16.0))
                        .frame(egui::Frame::NONE)
                        .desired_width(f32::INFINITY)
                        .margin(vec2(8.0, 8.0)),
                );
                search.request_focus();
                if search.changed() {
                    self.highlighted = 0;
                }
                ui.separator();

                if results.is_empty() {
                    ui.add_space(8.0);
                    ui.label(egui::RichText::new("No matches").color(p.muted));
                    ui.add_space(8.0);
                    return;
                }
                egui::ScrollArea::vertical()
                    .max_height(ROW_HEIGHT * 10.0)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        for (position, &index) in results.iter().enumerate() {
                            let highlighted = position == self.highlighted;
                            let response = row(ui, p, &entries[index], highlighted);
                            if highlighted && (up || down) {
                                response.scroll_to_me(None);
                            }
                            if response.hovered()
                                && ui.input(|i| i.pointer.delta() != egui::Vec2::ZERO)
                            {
                                self.highlighted = position;
                            }
                            if response.clicked() {
                                outcome = Some(Outcome::Chosen(index));
                            }
                        }
                    });
            });
        if outcome.is_none() && modal.should_close() {
            outcome = Some(Outcome::Closed);
        }
        outcome
    }
}

fn row(ui: &mut egui::Ui, p: &Palette, entry: &Entry, highlighted: bool) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), ROW_HEIGHT), Sense::click());
    let painter = ui.painter();
    if highlighted {
        painter.rect_filled(rect, CornerRadius::same(6), p.accent.gamma_multiply(0.18));
    }
    let detail_width = if entry.detail.is_empty() {
        0.0
    } else {
        let galley =
            painter.layout_no_wrap(entry.detail.clone(), FontId::proportional(12.0), p.muted);
        let width = galley.size().x;
        painter.galley(
            rect.right_center() - vec2(10.0 + width, galley.size().y / 2.0),
            galley,
            p.muted,
        );
        width + 20.0
    };
    let label = painter.layout_no_wrap(entry.label.clone(), FontId::proportional(14.0), p.text);
    let mut clip = rect;
    clip.max.x -= detail_width;
    painter.with_clip_rect(clip).galley(
        rect.left_center() + vec2(10.0, -label.size().y / 2.0),
        label,
        p.text,
    );
    response
}

/// Indices of the entries matching `query`, best first. An empty query
/// keeps the default order.
pub fn rank(query: &str, entries: &[Entry]) -> Vec<usize> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return (0..entries.len().min(MAX_RESULTS)).collect();
    }
    let mut scored: Vec<(i32, usize)> = entries
        .iter()
        .enumerate()
        .filter_map(|(i, e)| {
            let label = score(&query, &e.label.to_lowercase());
            // The detail ("Image", "Volume", …) counts, but less.
            let detail =
                score(&query, &format!("{} {}", e.label, e.detail).to_lowercase()).map(|s| s / 2);
            label.max(detail).map(|s| (s, i))
        })
        .collect();
    // Stable, so equal scores keep the default (recent-first) order.
    scored.sort_by_key(|&(s, _)| std::cmp::Reverse(s));
    scored
        .into_iter()
        .take(MAX_RESULTS)
        .map(|(_, i)| i)
        .collect()
}

/// Fuzzy match: every character of `query` must appear in `text` in order.
/// Consecutive characters, word starts and a plain substring match score
/// higher. Both are expected in lower case.
fn score(query: &str, text: &str) -> Option<i32> {
    let mut total = 0;
    let mut chars = text.char_indices().peekable();
    let mut previous: Option<usize> = None;
    let mut prev_char = ' ';
    for q in query.chars() {
        loop {
            let (index, c) = chars.next()?;
            let at_word_start = !prev_char.is_alphanumeric();
            prev_char = c;
            if c == q {
                total += 10;
                if previous.is_some_and(|p| p + 1 == index) {
                    total += 15;
                }
                if at_word_start {
                    total += 20;
                }
                if let Some(p) = previous {
                    total -= (index - p - 1).min(10) as i32;
                }
                previous = Some(index);
                break;
            }
        }
    }
    if text.contains(query) {
        total += 50;
    }
    if text.starts_with(query) {
        total += 30;
    }
    Some(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entries(labels: &[&str]) -> Vec<Entry> {
        labels
            .iter()
            .map(|l| Entry {
                label: (*l).into(),
                detail: String::new(),
            })
            .collect()
    }

    #[test]
    fn matches_in_order_only() {
        assert!(score("api", "api-server").is_some());
        assert!(score("asr", "api-server").is_some());
        assert!(score("rsa", "api-server").is_none());
    }

    #[test]
    fn prefers_prefix_and_word_starts() {
        let list = entries(&["Restart web-frontend", "Stop api-server", "api-server"]);
        assert_eq!(rank("api", &list)[0], 2);
        assert_eq!(rank("rw", &list)[0], 0);
    }

    #[test]
    fn empty_query_keeps_order() {
        let list = entries(&["b", "a"]);
        assert_eq!(rank("  ", &list), vec![0, 1]);
    }
}
