//! Timestamps in the formats the APIs use, without a date library.

use std::time::{SystemTime, UNIX_EPOCH};

/// Seconds since 1970.
pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Milliseconds since 1970.
pub fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// `2026-11-12T18:30:05Z`, as XML and JSON bodies carry times.
pub fn iso8601(secs: u64) -> String {
    let (y, mo, d) = civil(secs / 86_400);
    let rest = secs % 86_400;
    format!(
        "{y:04}-{mo:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
}

const DAYS: [&str; 7] = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"];
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// `2026-11-12T18:30:05.000Z`, from milliseconds, as S3 lists times.
pub fn iso8601_millis(ms: u64) -> String {
    let whole = iso8601(ms / 1000);
    format!("{}.{:03}Z", whole.trim_end_matches('Z'), ms % 1000)
}

/// `Thu, 12 Nov 2026 18:30:05 GMT`, as HTTP headers carry times.
pub fn http_date(secs: u64) -> String {
    let days = secs / 86_400;
    let (y, mo, d) = civil(days);
    let rest = secs % 86_400;
    format!(
        "{}, {d:02} {} {y:04} {:02}:{:02}:{:02} GMT",
        DAYS[(days % 7) as usize],
        MONTHS[(mo - 1) as usize],
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
}

/// Seconds since 1970 for a time written as [`http_date`] writes it, or
/// as [`iso8601`] does. None for anything else.
pub fn parse_date(text: &str) -> Option<u64> {
    let text = text.trim();
    let number = |part: &str| part.parse::<u64>().ok();
    let (y, mo, d, clock) = if let Some((_, rest)) = text.split_once(", ") {
        let mut parts = rest.split(' ');
        let d = number(parts.next()?)?;
        let name = parts.next()?;
        let mo = MONTHS.iter().position(|m| *m == name)? as u64 + 1;
        (number(parts.next()?)?, mo, d, parts.next()?)
    } else {
        let (date, clock) = text.split_once('T')?;
        let mut parts = date.split('-');
        let y = number(parts.next()?)?;
        (y, number(parts.next()?)?, number(parts.next()?)?, clock)
    };
    let mut clock = clock.trim_end_matches('Z').split(':');
    let h = number(clock.next()?)?;
    let mi = number(clock.next()?)?;
    let s = number(clock.next()?.split('.').next()?)?;
    if !(1..=12).contains(&mo) || !(1..=31).contains(&d) || y < 1970 || h > 23 || mi > 59 || s > 60
    {
        return None;
    }
    // Days from the date (Howard Hinnant's `days_from_civil`).
    let y = if mo <= 2 { y - 1 } else { y };
    let era = y / 400;
    let yoe = y % 400;
    let doy = (153 * (if mo > 2 { mo - 3 } else { mo + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = (era * 146_097 + doe).checked_sub(719_468)?;
    Some(days * 86_400 + h * 3600 + mi * 60 + s)
}

/// The date of a day counted from 1970-01-01 (Howard Hinnant's
/// `civil_from_days`).
fn civil(days: u64) -> (u64, u64, u64) {
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z % 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + u64::from(m <= 2), m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_dates() {
        assert_eq!(iso8601(0), "1970-01-01T00:00:00Z");
        assert_eq!(iso8601(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(iso8601(1_794_508_205), "2026-11-12T18:30:05Z");
        assert_eq!(
            iso8601_millis(1_794_508_205_042),
            "2026-11-12T18:30:05.042Z"
        );
    }

    #[test]
    fn writes_and_reads_http_dates() {
        assert_eq!(http_date(0), "Thu, 01 Jan 1970 00:00:00 GMT");
        assert_eq!(http_date(1_794_508_205), "Thu, 12 Nov 2026 18:30:05 GMT");
        for secs in [0, 951_782_400, 1_794_508_205, 4_102_444_799] {
            assert_eq!(parse_date(&http_date(secs)), Some(secs));
            assert_eq!(parse_date(&iso8601(secs)), Some(secs));
        }
        assert_eq!(parse_date("2026-11-12T18:30:05.042Z"), Some(1_794_508_205));
        assert_eq!(parse_date("yesterday"), None);
        assert_eq!(parse_date("Thu, 99 Nov 2026 18:30:05 GMT"), None);
    }
}
