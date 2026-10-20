//! CPU and memory limits, written the way `docker run --cpus/--memory`
//! takes them ("1.5", "512m", "2g").

/// Nano-CPUs per CPU, the unit the Engine uses.
const NANO: f64 = 1e9;
const KIB: i64 = 1024;
const MIB: i64 = KIB * 1024;
const GIB: i64 = MIB * 1024;

/// "1.5" → 1.5 CPUs in nano-CPUs. `None` if it isn't a positive number.
pub fn parse_cpus(text: &str) -> Option<i64> {
    let cpus: f64 = text.trim().parse().ok()?;
    (cpus.is_finite() && cpus > 0.0).then(|| (cpus * NANO).round() as i64)
}

/// "512m", "1.5g", "256MB" or plain bytes. `None` if it isn't positive or
/// the unit is unknown. Docker needs at least 6 MB, which it reports itself.
pub fn parse_memory(text: &str) -> Option<i64> {
    let text = text.trim().to_ascii_lowercase();
    let text = text.strip_suffix('b').unwrap_or(&text);
    let (number, unit) = match text.char_indices().last()? {
        (i, c) if c.is_ascii_alphabetic() => (&text[..i], c),
        _ => (text, ' '),
    };
    let unit = match unit {
        ' ' => 1,
        'k' => KIB,
        'm' => MIB,
        'g' => GIB,
        _ => return None,
    };
    let value: f64 = number.trim().parse().ok()?;
    let bytes = (value * unit as f64).round();
    (value.is_finite() && bytes >= 1.0).then_some(bytes as i64)
}

/// 1_500_000_000 → "1.5". Empty for no limit.
pub fn cpus_text(nano_cpus: i64) -> String {
    if nano_cpus <= 0 {
        return String::new();
    }
    let text = format!("{:.3}", nano_cpus as f64 / NANO);
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

/// 536_870_912 → "512m", in the largest unit that divides it evenly.
/// Empty for no limit.
pub fn memory_text(bytes: i64) -> String {
    match bytes {
        b if b <= 0 => String::new(),
        b if b % GIB == 0 => format!("{}g", b / GIB),
        b if b % MIB == 0 => format!("{}m", b / MIB),
        b if b % KIB == 0 => format!("{}k", b / KIB),
        b => b.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_cpus() {
        assert_eq!(parse_cpus("1.5"), Some(1_500_000_000));
        assert_eq!(parse_cpus(" 2 "), Some(2_000_000_000));
        assert_eq!(parse_cpus("0"), None);
        assert_eq!(parse_cpus("lots"), None);
        assert_eq!(cpus_text(1_500_000_000), "1.5");
        assert_eq!(cpus_text(2_000_000_000), "2");
        assert_eq!(cpus_text(0), "");
    }

    #[test]
    fn parses_memory() {
        assert_eq!(parse_memory("512m"), Some(512 * MIB));
        assert_eq!(parse_memory("512MB"), Some(512 * MIB));
        assert_eq!(parse_memory("1.5g"), Some(3 * GIB / 2));
        assert_eq!(parse_memory("4096"), Some(4096));
        assert_eq!(parse_memory("12x"), None);
        assert_eq!(parse_memory("0m"), None);
        assert_eq!(parse_memory(""), None);
        assert_eq!(memory_text(512 * MIB), "512m");
        assert_eq!(memory_text(2 * GIB), "2g");
        assert_eq!(memory_text(1536 * MIB), "1536m");
        assert_eq!(memory_text(0), "");
    }
}
