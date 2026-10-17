//! Live data for the one container whose details are open: its log output
//! and recent resource usage. Both are bounded so memory stays flat no
//! matter how chatty a container is.

use std::collections::VecDeque;

use crate::docker::StatsSample;

/// Lines of log output kept in memory.
pub const MAX_LOG_LINES: usize = 2_000;
/// Stats readings kept (about one per second).
pub const MAX_SAMPLES: usize = 60;

pub struct Live {
    pub container_id: String,
    pub logs: LogBuffer,
    pub stats: VecDeque<StatsSample>,
}

impl Live {
    pub fn new(container_id: String) -> Self {
        Self {
            container_id,
            logs: LogBuffer::default(),
            stats: VecDeque::with_capacity(MAX_SAMPLES),
        }
    }

    pub fn push_stats(&mut self, sample: StatsSample) {
        if self.stats.len() == MAX_SAMPLES {
            self.stats.pop_front();
        }
        self.stats.push_back(sample);
    }
}

/// Log text split into lines. Docker delivers output in arbitrary chunks,
/// so a chunk may end mid-line; the next chunk then continues that line.
#[derive(Default)]
pub struct LogBuffer {
    lines: VecDeque<String>,
    /// Whether the last line is still waiting for its newline.
    open: bool,
}

impl LogBuffer {
    pub fn push(&mut self, chunk: &str) {
        let mut pieces = chunk.split('\n');
        if let Some(first) = pieces.next() {
            match self.lines.back_mut() {
                Some(last) if self.open => last.push_str(first.trim_end_matches('\r')),
                _ => self
                    .lines
                    .push_back(first.trim_end_matches('\r').to_owned()),
            }
        }
        for piece in pieces {
            self.lines
                .push_back(piece.trim_end_matches('\r').to_owned());
        }
        // A chunk ending in '\n' leaves an empty "line" behind; that's the
        // start of the next line, not output.
        self.open = !chunk.ends_with('\n');
        if !self.open && self.lines.back().is_some_and(String::is_empty) {
            self.lines.pop_back();
        }
        while self.lines.len() > MAX_LOG_LINES {
            self.lines.pop_front();
        }
    }

    pub fn lines(&self) -> impl Iterator<Item = &str> {
        self.lines.iter().map(String::as_str)
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.lines.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(buf: &LogBuffer) -> Vec<&str> {
        buf.lines().collect()
    }

    #[test]
    fn joins_lines_split_across_chunks() {
        let mut buf = LogBuffer::default();
        buf.push("hello wo");
        buf.push("rld\nsecond\n");
        buf.push("third\r\n");
        assert_eq!(lines(&buf), vec!["hello world", "second", "third"]);
    }

    #[test]
    fn keeps_only_the_newest_lines() {
        let mut buf = LogBuffer::default();
        for i in 0..MAX_LOG_LINES + 10 {
            buf.push(&format!("line {i}\n"));
        }
        assert_eq!(buf.len(), MAX_LOG_LINES);
        assert_eq!(buf.lines().next(), Some("line 10"));
    }

    #[test]
    fn keeps_a_minute_of_stats() {
        let mut live = Live::new("x".into());
        for _ in 0..MAX_SAMPLES + 5 {
            live.push_stats(StatsSample::default());
        }
        assert_eq!(live.stats.len(), MAX_SAMPLES);
    }
}
