//! Human-friendly formatting for times and sizes.

use std::time::{SystemTime, UNIX_EPOCH};

/// Formats a unix timestamp as a coarse "N units ago" string.
pub fn time_ago(unix_secs: i64) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    format_age(now - unix_secs)
}

fn format_age(secs: i64) -> String {
    let (n, unit) = match secs.max(0) {
        s if s < 60 => return "just now".into(),
        s if s < 3_600 => (s / 60, "minute"),
        s if s < 86_400 => (s / 3_600, "hour"),
        s if s < 2_592_000 => (s / 86_400, "day"),
        s if s < 31_536_000 => (s / 2_592_000, "month"),
        s => (s / 31_536_000, "year"),
    };
    let plural = if n == 1 { "" } else { "s" };
    format!("{n} {unit}{plural} ago")
}

/// Formats a byte count the way Docker does (decimal units).
pub fn format_size(bytes: i64) -> String {
    const UNITS: [&str; 5] = ["B", "kB", "MB", "GB", "TB"];
    let mut value = bytes.max(0) as f64;
    let mut unit = 0;
    while value >= 1000.0 && unit < UNITS.len() - 1 {
        value /= 1000.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{value} {}", UNITS[0])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// "1 image", "3 images".
pub fn plural(count: usize, noun: &str) -> String {
    if count == 1 {
        format!("1 {noun}")
    } else if ["s", "x", "ch", "sh"].iter().any(|end| noun.ends_with(end)) {
        format!("{count} {noun}es")
    } else {
        format!("{count} {noun}s")
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn pluralises_es_nouns() {
        assert_eq!(plural(2, "process"), "2 processes");
        assert_eq!(plural(1, "process"), "1 process");
        assert_eq!(plural(3, "image"), "3 images");
    }

    use super::*;

    #[test]
    fn formats_age() {
        assert_eq!(format_age(5), "just now");
        assert_eq!(format_age(60), "1 minute ago");
        assert_eq!(format_age(7_200), "2 hours ago");
        assert_eq!(format_age(86_400 * 3), "3 days ago");
        assert_eq!(format_age(-10), "just now");
    }

    #[test]
    fn formats_size() {
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(1_500), "1.5 kB");
        assert_eq!(format_size(187_300_000), "187.3 MB");
        assert_eq!(format_size(2_000_000_000), "2.0 GB");
    }

    #[test]
    fn pluralizes() {
        assert_eq!(plural(1, "image"), "1 image");
        assert_eq!(plural(0, "image"), "0 images");
        assert_eq!(plural(4, "volume"), "4 volumes");
    }
}
