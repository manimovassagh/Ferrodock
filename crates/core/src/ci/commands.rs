//! What steps tell the runner: workflow commands in their output
//! (`::group::`, `::error::`, `::add-mask::`, …) and the files `GITHUB_ENV`,
//! `GITHUB_OUTPUT` and `GITHUB_PATH`.

use super::{LineKind, LogLine};

/// Reads a step's output line by line, hiding masked values.
#[derive(Debug, Default)]
pub struct Output {
    /// Values never shown: secrets, and what `::add-mask::` named.
    masks: Vec<String>,
    /// From the old `::set-output name=…::` command.
    pub outputs: Vec<(String, String)>,
}

impl Output {
    pub fn new(masks: Vec<String>) -> Self {
        let mut masks: Vec<String> = masks.into_iter().filter(|m| !m.trim().is_empty()).collect();
        // Longest first, so a secret that holds another is hidden whole.
        masks.sort_by_key(|m| std::cmp::Reverse(m.len()));
        Self {
            masks,
            outputs: Vec::new(),
        }
    }

    pub fn mask(&mut self, value: &str) {
        if !value.trim().is_empty() && !self.masks.iter().any(|m| m == value) {
            self.masks.push(value.to_owned());
            self.masks.sort_by_key(|m| std::cmp::Reverse(m.len()));
        }
    }

    /// `text` with every masked value replaced by `***`.
    pub fn hide(&self, text: &str) -> String {
        let mut text = text.to_owned();
        for mask in &self.masks {
            text = text.replace(mask.as_str(), "***");
        }
        text
    }

    /// A line the step printed: shown, or a command handled (`None`).
    pub fn line(&mut self, raw: &str) -> Option<LogLine> {
        let raw = raw.trim_end_matches(['\r', '\n']);
        let shown = |text: &str, kind| LogLine {
            text: text.to_owned(),
            kind,
        };
        if let Some(rest) = raw.strip_prefix("##[") {
            // The older `##[group]Title` form.
            let (command, text) = rest.split_once(']').unwrap_or((rest, ""));
            let text = self.hide(text);
            return match command {
                "group" => Some(shown(&text, LineKind::Group)),
                "endgroup" => Some(shown("", LineKind::EndGroup)),
                "error" => Some(shown(&text, LineKind::Error)),
                "warning" => Some(shown(&text, LineKind::Warning)),
                _ => Some(shown(&self.hide(raw), LineKind::Output)),
            };
        }
        let Some(rest) = raw.trim_start().strip_prefix("::") else {
            return Some(shown(&self.hide(raw), LineKind::Output));
        };
        let Some((head, message)) = rest.split_once("::") else {
            return Some(shown(&self.hide(raw), LineKind::Output));
        };
        let (command, properties) = head.split_once(' ').unwrap_or((head, ""));
        let message = unescape(message);
        match command {
            "group" => Some(shown(&self.hide(&message), LineKind::Group)),
            "endgroup" => Some(shown("", LineKind::EndGroup)),
            "error" | "warning" | "notice" => {
                let kind = match command {
                    "error" => LineKind::Error,
                    "warning" => LineKind::Warning,
                    _ => LineKind::Notice,
                };
                let place = place(properties);
                let text = if place.is_empty() {
                    message
                } else {
                    format!("{place}: {message}")
                };
                Some(shown(&self.hide(&text), kind))
            }
            "add-mask" => {
                self.mask(&message);
                None
            }
            "set-output" => {
                if let Some(name) = property(properties, "name") {
                    self.outputs.push((name, message));
                }
                None
            }
            "debug" | "save-state" | "echo" | "stop-commands" | "add-matcher"
            | "remove-matcher" => None,
            _ => Some(shown(&self.hide(raw), LineKind::Output)),
        }
    }
}

/// `file=src/main.rs,line=3` as `src/main.rs:3`.
fn place(properties: &str) -> String {
    match (property(properties, "file"), property(properties, "line")) {
        (Some(file), Some(line)) => format!("{file}:{line}"),
        (Some(file), None) => file,
        _ => String::new(),
    }
}

fn property(properties: &str, name: &str) -> Option<String> {
    properties.split(',').find_map(|p| {
        let (k, v) = p.split_once('=')?;
        (k.trim() == name).then(|| unescape(v.trim()))
    })
}

/// Workflow commands escape `%`, CR and LF.
fn unescape(text: &str) -> String {
    text.replace("%0D", "\r")
        .replace("%0A", "\n")
        .replace("%3A", ":")
        .replace("%2C", ",")
        .replace("%25", "%")
}

/// The `KEY=value` and `KEY<<DELIMITER … DELIMITER` entries of a
/// `GITHUB_ENV` or `GITHUB_OUTPUT` file.
pub fn parse_file(text: &str) -> Result<Vec<(String, String)>, String> {
    let mut entries = Vec::new();
    let mut lines = text.lines();
    while let Some(line) = lines.next() {
        if line.trim().is_empty() {
            continue;
        }
        if let Some((key, delimiter)) = line.split_once("<<")
            && !key.contains('=')
        {
            let mut value = Vec::new();
            loop {
                match lines.next() {
                    Some(l) if l == delimiter => break,
                    Some(l) => value.push(l),
                    None => return Err(format!("{key} isn't ended by {delimiter}")),
                }
            }
            entries.push((key.to_owned(), value.join("\n")));
        } else if let Some((key, value)) = line.split_once('=') {
            entries.push((key.to_owned(), value.to_owned()));
        } else {
            return Err(format!("“{line}” isn't KEY=value"));
        }
    }
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_workflow_commands() {
        let mut out = Output::new(vec!["s3cret".into()]);
        assert_eq!(out.line("::group::Build").unwrap().kind, LineKind::Group);
        assert_eq!(out.line("::endgroup::").unwrap().kind, LineKind::EndGroup);
        let error = out
            .line("::error file=src/a.rs,line=3::Bad%0Athing")
            .unwrap();
        assert_eq!(error.kind, LineKind::Error);
        assert_eq!(error.text, "src/a.rs:3: Bad\nthing");
        assert_eq!(
            out.line("::warning::careful").unwrap().kind,
            LineKind::Warning
        );
        assert_eq!(out.line("token is s3cret").unwrap().text, "token is ***");
        assert!(out.line("::add-mask::hunter2").is_none());
        assert_eq!(out.line("pw hunter2").unwrap().text, "pw ***");
        assert!(out.line("::set-output name=v::1.0").is_none());
        assert_eq!(out.outputs, [("v".into(), "1.0".into())]);
        assert!(out.line("::debug::quiet").is_none());
        assert_eq!(
            out.line("##[group]Old style").unwrap().kind,
            LineKind::Group
        );
        assert_eq!(out.line("a :: b").unwrap().text, "a :: b");
    }

    #[test]
    fn reads_env_and_output_files() {
        let text = "A=1\nB=x=y\n\nNOTES<<EOF\nline 1\nline 2\nEOF\nC=\n";
        assert_eq!(
            parse_file(text).unwrap(),
            [
                ("A".into(), "1".into()),
                ("B".into(), "x=y".into()),
                ("NOTES".into(), "line 1\nline 2".into()),
                ("C".into(), String::new()),
            ]
        );
        assert!(parse_file("X<<END\nno end\n").is_err());
        assert!(parse_file("just words\n").is_err());
    }
}
