//! Turns log lines with ANSI colour codes into styled text, and highlights
//! search matches.

use eframe::egui::{Color32, FontId, TextFormat, text::LayoutJob};

use crate::theme::Palette;

/// One run of text in a single style.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span<'a> {
    pub text: &'a str,
    /// The ANSI colour (0–15), if one is set.
    pub color: Option<u8>,
    pub bold: bool,
}

/// Splits `line` into styled spans, dropping the escape codes. Codes other
/// than colours (cursor movement and the like) are removed too.
pub fn parse(line: &str) -> Vec<Span<'_>> {
    let mut spans = Vec::new();
    let mut color = None;
    let mut bold = false;
    let mut rest = line;
    while let Some(start) = rest.find('\x1b') {
        if start > 0 {
            spans.push(Span {
                text: &rest[..start],
                color,
                bold,
            });
        }
        let after = &rest[start + 1..];
        let Some(body) = after.strip_prefix('[') else {
            // A lone escape: drop it.
            rest = after;
            continue;
        };
        // A CSI sequence ends with a byte in @..~.
        let end = body
            .char_indices()
            .find(|(_, c)| ('@'..='~').contains(c))
            .map(|(i, _)| i);
        let Some(end) = end else {
            rest = "";
            break;
        };
        if body[end..].starts_with('m') {
            apply_sgr(&body[..end], &mut color, &mut bold);
        }
        rest = &body[end + 1..];
    }
    if !rest.is_empty() {
        spans.push(Span {
            text: rest,
            color,
            bold,
        });
    }
    spans
}

/// Applies "select graphic rendition" parameters, e.g. `1;31`.
fn apply_sgr(params: &str, color: &mut Option<u8>, bold: &mut bool) {
    if params.is_empty() {
        *color = None;
        *bold = false;
        return;
    }
    for code in params.split(';').filter_map(|p| p.parse::<u8>().ok()) {
        match code {
            0 => {
                *color = None;
                *bold = false;
            }
            1 => *bold = true,
            22 => *bold = false,
            30..=37 => *color = Some(code - 30),
            90..=97 => *color = Some(code - 90 + 8),
            39 => *color = None,
            _ => {}
        }
    }
}

/// The text without any escape codes.
pub fn strip(line: &str) -> String {
    parse(line).into_iter().map(|s| s.text).collect()
}

/// Terminal colours, tuned to stay readable on both themes.
pub fn ansi_color(p: &Palette, index: u8) -> Color32 {
    let dark = p.bg.r() < 128;
    match index % 8 {
        0 => p.muted,
        1 => p.danger,
        2 => p.success,
        3 => p.warning,
        4 => {
            if dark {
                Color32::from_rgb(0x6c, 0xa8, 0xff)
            } else {
                Color32::from_rgb(0x1f, 0x5f, 0xd1)
            }
        }
        5 => {
            if dark {
                Color32::from_rgb(0xd0, 0x8c, 0xff)
            } else {
                Color32::from_rgb(0x8a, 0x3f, 0xc7)
            }
        }
        6 => {
            if dark {
                Color32::from_rgb(0x4f, 0xd1, 0xd9)
            } else {
                Color32::from_rgb(0x0b, 0x7f, 0x8a)
            }
        }
        _ => p.text,
    }
}

/// A log line as styled text: ANSI colours applied, lines that look like
/// errors or warnings tinted when they carry no colours of their own, and
/// `highlight` (lowercase) marked wherever it occurs.
pub fn layout(p: &Palette, line: &str, highlight: &str, font: FontId) -> LayoutJob {
    let spans = parse(line);
    let plain: String = spans.iter().map(|s| s.text).collect();
    let tint = if spans.iter().all(|s| s.color.is_none()) {
        level_color(p, &plain)
    } else {
        None
    };
    let lower = plain.to_lowercase();
    let matches: Vec<(usize, usize)> = if highlight.is_empty() {
        Vec::new()
    } else {
        lower
            .match_indices(highlight)
            .map(|(i, m)| (i, i + m.len()))
            .collect()
    };

    let mut job = LayoutJob::default();
    let mut offset = 0;
    for span in spans {
        let color = span
            .color
            .map(|c| ansi_color(p, c))
            .or(tint)
            .unwrap_or(p.text);
        // Split the span where highlights start and end.
        let mut cuts = vec![0, span.text.len()];
        for &(a, b) in &matches {
            for edge in [a, b] {
                if edge > offset && edge < offset + span.text.len() {
                    cuts.push(edge - offset);
                }
            }
        }
        cuts.sort_unstable();
        cuts.dedup();
        for pair in cuts.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            // Byte offsets can fall inside a character when lowercasing
            // changed lengths; skip highlighting rather than split it.
            let Some(text) = span.text.get(a..b) else {
                continue;
            };
            let highlighted = matches
                .iter()
                .any(|&(m0, m1)| offset + a >= m0 && offset + b <= m1);
            job.append(
                text,
                0.0,
                TextFormat {
                    font_id: font.clone(),
                    color,
                    background: if highlighted {
                        p.warning.gamma_multiply(0.35)
                    } else {
                        Color32::TRANSPARENT
                    },
                    ..Default::default()
                },
            );
        }
        offset += span.text.len();
    }
    job
}

/// A colour for lines that look like errors or warnings.
fn level_color(p: &Palette, line: &str) -> Option<Color32> {
    let upper = line.to_uppercase();
    if ["ERROR", "FATAL", "PANIC", "EXCEPTION"]
        .iter()
        .any(|w| upper.contains(w))
    {
        Some(p.danger)
    } else if upper.contains("WARN") {
        Some(p.warning)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_colours_and_resets() {
        let spans = parse("\x1b[1;31mERR\x1b[0m done");
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].text, "ERR");
        assert_eq!(spans[0].color, Some(1));
        assert!(spans[0].bold);
        assert_eq!(spans[1].text, " done");
        assert_eq!(spans[1].color, None);
    }

    #[test]
    fn bright_colours_and_other_codes() {
        let spans = parse("\x1b[92mok\x1b[2K\x1b[39m!");
        assert_eq!(spans[0].color, Some(10));
        assert_eq!(strip("\x1b[92mok\x1b[2K\x1b[39m!"), "ok!");
        // An unfinished sequence at the end is dropped.
        assert_eq!(strip("tail\x1b[3"), "tail");
    }

    #[test]
    fn plain_text_is_one_span() {
        assert_eq!(
            parse("hello"),
            vec![Span {
                text: "hello",
                color: None,
                bold: false
            }]
        );
    }

    #[test]
    fn highlights_matches() {
        let p = &crate::theme::LIGHT;
        let job = layout(p, "GET /api 200", "api", FontId::monospace(12.0));
        let highlighted: Vec<&str> = job
            .sections
            .iter()
            .filter(|s| s.format.background != Color32::TRANSPARENT)
            .map(|s| &job.text[s.byte_range.start.0..s.byte_range.end.0])
            .collect();
        assert_eq!(highlighted, vec!["api"]);
    }
}
