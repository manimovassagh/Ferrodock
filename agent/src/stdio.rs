//! The engine offered over standard input and output: how Ferrodock
//! reaches it on Windows, where the agent runs in a WSL distro started by
//! `wsl.exe`. Many connections share the one stream (see `mux`); each is
//! joined to the engine's Unix socket.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex};
use std::thread;

#[path = "../../src/vm/mux.rs"]
mod mux;

/// Data for a connection, or None once Ferrodock has sent everything.
type Inbox = Sender<Option<Vec<u8>>>;

/// Serves connections from `input` until it ends (Ferrodock quit or
/// stopped the engine), writing the engine's side to `output`.
pub fn run(mut input: impl Read, output: impl Write + Send + 'static, socket: &Path) {
    let output = Arc::new(Mutex::new(output));
    let mut connections: HashMap<u32, Inbox> = HashMap::new();
    loop {
        let frame = match mux::read(&mut input) {
            Ok(Some(frame)) => frame,
            Ok(None) => return,
            Err(e) => {
                crate::log(&format!("stdin: {e}"));
                return;
            }
        };
        match frame {
            (id, mux::OPEN, _) => {
                if let Some(inbox) = open(id, socket.to_path_buf(), output.clone()) {
                    connections.insert(id, inbox);
                }
            }
            (id, mux::DATA, data) => {
                if let Some(inbox) = connections.get(&id) {
                    let _ = inbox.send(Some(data));
                }
            }
            (id, _, _) => {
                if let Some(inbox) = connections.remove(&id) {
                    let _ = inbox.send(None);
                }
            }
        }
    }
}

/// Sends one frame to Ferrodock.
fn send(output: &Mutex<impl Write>, id: u32, kind: u8, data: &[u8]) -> bool {
    let Ok(mut out) = output.lock() else {
        return false;
    };
    out.write_all(&mux::encode(id, kind, data)).is_ok() && out.flush().is_ok()
}

/// Connects to the engine for connection `id`: one thread carries the
/// engine's replies out, another writes what Ferrodock sends, so a slow
/// connection never holds up the others.
fn open(
    id: u32,
    socket: PathBuf,
    output: Arc<Mutex<impl Write + Send + 'static>>,
) -> Option<Inbox> {
    let engine = match UnixStream::connect(&socket) {
        Ok(engine) => engine,
        Err(_) => {
            // Not up yet: Ferrodock sees the connection close and retries.
            send(&output, id, mux::CLOSE, b"");
            return None;
        }
    };
    let (mut reader, mut writer) = (engine.try_clone().ok()?, engine);
    thread::spawn(move || {
        let mut buffer = vec![0u8; 64 * 1024];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if !send(&output, id, mux::DATA, &buffer[..n]) {
                        break;
                    }
                }
            }
        }
        send(&output, id, mux::CLOSE, b"");
    });
    let (inbox, messages) = channel::<Option<Vec<u8>>>();
    thread::spawn(move || {
        for message in messages {
            match message {
                Some(data) => {
                    if writer.write_all(&data).is_err() {
                        break;
                    }
                }
                None => break,
            }
        }
        let _ = writer.shutdown(Shutdown::Write);
    });
    Some(inbox)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A writer the test can read back.
    #[derive(Clone, Default)]
    struct Captured(Arc<Mutex<Vec<u8>>>);

    impl Write for Captured {
        fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(data);
            Ok(data.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    /// Two connections to an "engine" that answers in upper case.
    #[test]
    fn carries_connections_to_the_engine() {
        let dir = std::env::temp_dir().join(format!("ferrodock-stdio-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let socket = dir.join("engine.sock");
        let engine = std::os::unix::net::UnixListener::bind(&socket).unwrap();
        thread::spawn(move || {
            for stream in engine.incoming() {
                let mut stream = stream.unwrap();
                thread::spawn(move || {
                    let mut text = String::new();
                    stream.read_to_string(&mut text).unwrap();
                    stream.write_all(text.to_uppercase().as_bytes()).unwrap();
                });
            }
        });

        let mut input = Vec::new();
        for (id, text) in [(1, "one"), (2, "two")] {
            input.extend(mux::encode(id, mux::OPEN, b""));
            input.extend(mux::encode(id, mux::DATA, text.as_bytes()));
            input.extend(mux::encode(id, mux::CLOSE, b""));
        }
        let output = Captured::default();
        run(input.as_slice(), output.clone(), &socket);

        // Both replies arrive, each followed by its close.
        let mut replies: HashMap<u32, (String, bool)> = HashMap::new();
        for _ in 0..200 {
            replies.clear();
            let bytes = output.0.lock().unwrap().clone();
            let mut frames = bytes.as_slice();
            while let Ok(Some((id, kind, data))) = mux::read(&mut frames) {
                let reply = replies.entry(id).or_default();
                reply.0.push_str(&String::from_utf8_lossy(&data));
                reply.1 |= kind == mux::CLOSE;
            }
            if replies.values().filter(|r| r.1).count() == 2 {
                break;
            }
            thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(replies[&1], ("ONE".into(), true));
        assert_eq!(replies[&2], ("TWO".into(), true));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
