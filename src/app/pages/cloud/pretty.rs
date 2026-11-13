//! Bodies made readable: JSON and XML indented, a form body one field
//! per line. Anything else is shown as it came.

/// `text` laid out for reading, by what it looks like.
pub fn body(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed)
            && let Ok(pretty) = serde_json::to_string_pretty(&value)
        {
            return pretty;
        }
    } else if trimmed.starts_with('<') {
        return xml(trimmed);
    } else if looks_like_form(trimmed) {
        return form(trimmed);
    }
    text.to_owned()
}

fn looks_like_form(text: &str) -> bool {
    !text.contains(['\n', ' ', '<', '{'])
        && text
            .split('&')
            .all(|pair| pair.split_once('=').is_some_and(|(k, _)| !k.is_empty()))
}

/// `A=1&B=x%20y` as `A = 1` and `B = x y`, one per line.
fn form(text: &str) -> String {
    text.split('&')
        .filter_map(|pair| pair.split_once('='))
        .map(|(key, value)| format!("{} = {}", decode(key), decode(value)))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Percent-decoding, with `+` for a space, as forms are sent.
fn decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = |b: u8| (b as char).to_digit(16);
        match bytes[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < bytes.len()
                && hex(bytes[i + 1]).is_some()
                && hex(bytes[i + 2]).is_some() =>
            {
                let value = hex(bytes[i + 1]).unwrap_or(0) * 16 + hex(bytes[i + 2]).unwrap_or(0);
                out.push(value as u8);
                i += 2;
            }
            b => out.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// XML with each element on its own line, indented by depth. An element
/// holding only text stays on one line.
fn xml(text: &str) -> String {
    // Tags and the text between them, in order.
    let mut parts: Vec<&str> = Vec::new();
    let mut rest = text;
    while let Some(open) = rest.find('<') {
        let before = rest[..open].trim();
        if !before.is_empty() {
            parts.push(before);
        }
        let Some(close) = rest[open..].find('>') else {
            break;
        };
        parts.push(&rest[open..=open + close]);
        rest = &rest[open + close + 1..];
    }
    let tail = rest.trim();
    if !tail.is_empty() {
        parts.push(tail);
    }

    let opens = |p: &str| {
        p.starts_with('<') && !p.starts_with("</") && !p.starts_with("<?") && !p.ends_with("/>")
    };
    let closes = |p: &str| p.starts_with("</");
    let mut out = String::new();
    let mut depth = 0usize;
    let mut i = 0;
    while i < parts.len() {
        let part = parts[i];
        if closes(part) {
            depth = depth.saturating_sub(1);
        }
        out.push_str(&"  ".repeat(depth));
        // <a>text</a> on one line.
        if opens(part)
            && i + 2 < parts.len()
            && !parts[i + 1].starts_with('<')
            && closes(parts[i + 2])
        {
            out.push_str(part);
            out.push_str(parts[i + 1]);
            out.push_str(parts[i + 2]);
            i += 3;
        } else if opens(part) && i + 1 < parts.len() && closes(parts[i + 1]) {
            out.push_str(part);
            out.push_str(parts[i + 1]);
            i += 2;
        } else {
            out.push_str(part);
            if opens(part) {
                depth += 1;
            }
            i += 1;
        }
        out.push('\n');
    }
    out.trim_end().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indents_json() {
        assert_eq!(
            body(r#"{"a":1,"b":["x"]}"#),
            "{\n  \"a\": 1,\n  \"b\": [\n    \"x\"\n  ]\n}"
        );
        // Cut short, so not JSON any more: as it came.
        assert_eq!(body(r#"{"a":1,"b"#), r#"{"a":1,"b"#);
    }

    #[test]
    fn indents_xml() {
        let text = "<GetRoleResponse xmlns=\"x\"><GetRoleResult><Role><RoleName>app</RoleName><Tags></Tags>\
                    <RoleLastUsed/></Role></GetRoleResult></GetRoleResponse>";
        assert_eq!(
            body(text),
            "<GetRoleResponse xmlns=\"x\">\n  <GetRoleResult>\n    <Role>\n      <RoleName>app</RoleName>\n      \
             <Tags></Tags>\n      <RoleLastUsed/>\n    </Role>\n  </GetRoleResult>\n</GetRoleResponse>"
        );
    }

    #[test]
    fn splits_forms() {
        assert_eq!(
            body("Action=CreateRole&RoleName=my+app&AssumeRolePolicyDocument=%7B%22V%22%3A1%7D"),
            "Action = CreateRole\nRoleName = my app\nAssumeRolePolicyDocument = {\"V\":1}"
        );
        assert_eq!(body("plain words"), "plain words");
        assert_eq!(body("100%"), "100%");
    }
}
