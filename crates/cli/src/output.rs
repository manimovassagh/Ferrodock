//! Printing results: aligned tables for people, compact JSON with `--json`.

use std::io::{IsTerminal, Write};

use serde::Serialize;

/// Prints `value` as one line of JSON.
pub fn json(value: &impl Serialize) -> Result<(), String> {
    let text = serde_json::to_string(value).map_err(|e| e.to_string())?;
    println!("{text}");
    Ok(())
}

/// A progress line on stderr, rewritten in place. Only on a terminal:
/// piped or logged output gets no progress, just the result.
pub struct Progress {
    shown: Option<String>,
}

impl Progress {
    pub fn new() -> Self {
        Self {
            shown: std::io::stderr().is_terminal().then(String::new),
        }
    }

    pub fn show(&mut self, line: String) {
        if let Some(shown) = &mut self.shown
            && *shown != line
        {
            // Pad over what's left of a longer previous line.
            let pad = shown.chars().count().saturating_sub(line.chars().count());
            eprint!("\r{line}{}", " ".repeat(pad));
            let _ = std::io::stderr().flush();
            *shown = line;
        }
    }

    /// Ends the line, if one was shown.
    pub fn done(&self) {
        if self.shown.as_ref().is_some_and(|s| !s.is_empty()) {
            eprintln!();
        }
    }
}

/// `rows` under `headers`, in columns padded to their widest cell. Lines
/// don't end in spaces.
pub fn table(headers: &[&str], rows: &[Vec<String>]) -> String {
    let mut widths: Vec<usize> = headers.iter().map(|h| h.chars().count()).collect();
    for row in rows {
        for (width, cell) in widths.iter_mut().zip(row) {
            *width = (*width).max(cell.chars().count());
        }
    }
    let line = |cells: Vec<&str>| {
        let last = cells.len().saturating_sub(1);
        cells
            .iter()
            .enumerate()
            .map(|(i, cell)| {
                if i == last {
                    (*cell).to_owned()
                } else {
                    format!("{cell:<width$}", width = widths[i])
                }
            })
            .collect::<Vec<_>>()
            .join("  ")
            .trim_end()
            .to_owned()
    };
    let mut out = line(headers.to_vec());
    for row in rows {
        out.push('\n');
        out.push_str(&line(row.iter().map(String::as_str).collect()));
    }
    out
}

/// Prints the table, or `empty` when there are no rows.
pub fn print_table(headers: &[&str], rows: &[Vec<String>], empty: &str) {
    if rows.is_empty() {
        println!("{empty}");
    } else {
        println!("{}", table(headers, rows));
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn lines_up_columns() {
        let rows = vec![
            vec!["web".to_owned(), "nginx:alpine".to_owned(), "Up".to_owned()],
            vec!["database".to_owned(), "postgres".to_owned(), "".to_owned()],
        ];
        assert_eq!(
            super::table(&["NAME", "IMAGE", "STATUS"], &rows),
            "NAME      IMAGE         STATUS\n\
             web       nginx:alpine  Up\n\
             database  postgres      "
                .trim_end()
        );
    }
}
