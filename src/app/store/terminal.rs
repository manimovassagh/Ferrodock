//! The open terminal: which session it is, and its screen.

use crate::docker::TermOp;

/// Lines kept above the screen.
const SCROLLBACK: usize = 2000;

pub struct Terminal {
    pub session: u64,
    pub container_id: String,
    pub parser: vt100::Parser,
    /// Set when the shell ended; the message says why, if it wasn't a
    /// normal exit.
    pub ended: Option<Option<String>>,
    /// It ended because the container has no shell.
    pub no_shell: bool,
    /// A debug shell, running in a helper next to the container.
    pub debug: bool,
}

/// Records what a terminal request changes on screen. Returns the session
/// to close, when opening a new one replaces it.
pub fn requested(term: &mut Option<Terminal>, op: &TermOp) -> Option<u64> {
    match op {
        TermOp::Open {
            session,
            container,
            debug,
            cols,
            rows,
            ..
        } => {
            let replaced = term.take().map(|t| t.session);
            *term = Some(Terminal {
                session: *session,
                container_id: container.clone(),
                parser: vt100::Parser::new(*rows, *cols, SCROLLBACK),
                ended: None,
                no_shell: false,
                debug: *debug,
            });
            replaced
        }
        TermOp::Resize {
            session,
            cols,
            rows,
        } => {
            if let Some(t) = term.as_mut().filter(|t| t.session == *session) {
                t.parser.screen_mut().set_size(*rows, *cols);
            }
            None
        }
        TermOp::Close { session } => {
            // The screen stays, marked as ended, until a new session opens.
            if let Some(t) = term.as_mut().filter(|t| t.session == *session) {
                t.ended.get_or_insert(None);
            }
            None
        }
        TermOp::Input { .. } => None,
    }
}

pub fn output(term: &mut Option<Terminal>, session: u64, bytes: &[u8]) {
    if let Some(t) = term.as_mut().filter(|t| t.session == session) {
        t.parser.process(bytes);
    }
}

pub fn exited(term: &mut Option<Terminal>, session: u64, message: Option<String>, no_shell: bool) {
    if let Some(t) = term.as_mut().filter(|t| t.session == session) {
        t.ended = Some(message);
        t.no_shell = no_shell;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn follows_its_session() {
        let mut term = None;
        let open = |session| TermOp::Open {
            session,
            container: "c1".into(),
            shell: String::new(),
            debug: false,
            cols: 20,
            rows: 4,
        };
        assert_eq!(requested(&mut term, &open(1)), None);
        output(&mut term, 1, b"hello");
        output(&mut term, 9, b"not mine");
        let screen = term.as_ref().unwrap().parser.screen().contents();
        assert_eq!(screen.trim(), "hello");
        // A new session replaces the old one, which should be closed.
        assert_eq!(requested(&mut term, &open(2)), Some(1));
        exited(&mut term, 2, Some("gone".into()), false);
        assert_eq!(term.as_ref().unwrap().ended, Some(Some("gone".into())));
    }
}
