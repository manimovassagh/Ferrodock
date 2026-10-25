//! One Ferrodock at a time, per user.
//!
//! The Ferrodock that runs holds an exclusive lock on `app.lock` in its
//! folder (`~/.ferrodock`, `%LOCALAPPDATA%\Ferrodock`) until its process
//! exits, and listens on a loopback port it writes to `app.port`. Another
//! one started later finds the lock taken, asks the first, through that
//! port, to bring its window forward, and exits. After an update the new
//! version waits for the lock instead: the old one is still stopping its
//! engine.

use std::fs::{self, File, TryLockError};
use std::hash::{BuildHasher, RandomState};
use std::io::{self, BufRead, BufReader, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

/// Passed to the new version when an update restarts Ferrodock.
pub const AFTER_UPDATE_FLAG: &str = "--after-update";

/// How long the new version waits for the old one to quit after an
/// update. The old one stops its engine first: usually a few seconds,
/// but on Windows up to 20 for containers to stop, then WSL's own
/// shutdown, so this leaves plenty of room over that.
const HANDOFF: Duration = Duration::from_secs(120);

/// How long to keep trying to reach the Ferrodock that runs: it may have
/// just taken the lock and not written its port yet.
const WAKE: Duration = Duration::from_secs(2);

/// How often a wait tries again. Only while starting.
const RETRY: Duration = Duration::from_millis(100);

/// The running Ferrodock's claim: the lock, held until the process exits,
/// and where others reach it.
pub struct Instance {
    _lock: File,
    listener: TcpListener,
    /// Sent along with each request, so only someone who can read
    /// `app.port` (this user) is listened to.
    token: String,
}

/// What a starting Ferrodock does.
pub enum Start {
    /// Runs. Without an instance when there's no folder to lock in: then
    /// it runs unguarded, as two Ferrodocks are better than none.
    Run(Option<Instance>),
    /// Another Ferrodock runs, and was asked to show its window.
    Exit,
}

/// Claims this user's Ferrodock, or hands over to the one that runs.
/// Opened at login (`background`), the other one isn't brought forward.
pub fn start(background: bool) -> Start {
    let Some(dir) = crate::vm::Paths::locate().map(|p| p.dir) else {
        return Start::Run(None);
    };
    let after_update = std::env::args().any(|a| a == AFTER_UPDATE_FLAG);
    let wait = if after_update {
        HANDOFF
    } else {
        Duration::ZERO
    };
    match claim(&dir, wait) {
        Ok(Some(instance)) => Start::Run(Some(instance)),
        // The old version is still quitting, so there's no one to hand
        // over to: exiting now would leave no Ferrodock at all.
        Ok(None) if after_update => Start::Run(None),
        Ok(None) => {
            if !background {
                wake(&dir);
            }
            Start::Exit
        }
        Err(_) => Start::Run(None),
    }
}

/// Takes the lock in `dir`, waiting up to `wait` for another Ferrodock to
/// let go of it. None when another one keeps it.
fn claim(dir: &Path, wait: Duration) -> io::Result<Option<Instance>> {
    crate::vm::private_dir(dir)?;
    let lock = File::options()
        .create(true)
        .truncate(false)
        .write(true)
        .open(dir.join("app.lock"))?;
    let deadline = Instant::now() + wait;
    loop {
        match lock.try_lock() {
            Ok(()) => break,
            Err(TryLockError::WouldBlock) if Instant::now() < deadline => thread::sleep(RETRY),
            Err(TryLockError::WouldBlock) => return Ok(None),
            Err(TryLockError::Error(e)) => return Err(e),
        }
    }
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
    let token = format!(
        "{:016x}{:016x}",
        RandomState::new().hash_one(std::process::id()),
        RandomState::new().hash_one(Instant::now())
    );
    // Written whole, then moved in place, so it's never read half done;
    // readable by this user only, as the token is what lets others in.
    let port = dir.join("app.port");
    let partial = dir.join("app.port.partial");
    let _ = fs::remove_file(&partial);
    let mut options = File::options();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options
        .open(&partial)?
        .write_all(format!("{} {token}", listener.local_addr()?.port()).as_bytes())?;
    fs::rename(&partial, &port)?;
    Ok(Some(Instance {
        _lock: lock,
        listener,
        token,
    }))
}

impl Instance {
    /// Calls `show` whenever another Ferrodock asks for the window. The
    /// thread sleeps in `accept` in between.
    pub fn serve(&self, show: impl Fn() + Send + 'static) -> io::Result<()> {
        let listener = self.listener.try_clone()?;
        let expected = format!("show {}", self.token);
        thread::Builder::new()
            .name("instance".into())
            .spawn(move || {
                for stream in listener.incoming().flatten() {
                    // A client that never writes doesn't hold up the next.
                    let _ = stream.set_read_timeout(Some(Duration::from_secs(1)));
                    let mut line = String::new();
                    if BufReader::new(stream).read_line(&mut line).is_ok()
                        && same(line.trim_end().as_bytes(), expected.as_bytes())
                    {
                        show();
                    }
                }
            })?;
        Ok(())
    }
}

/// Asks the Ferrodock that holds the lock in `dir` to show its window.
/// Whether it was reached.
fn wake(dir: &Path) -> bool {
    let deadline = Instant::now() + WAKE;
    loop {
        if send_show(dir).is_ok() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        thread::sleep(RETRY);
    }
}

fn send_show(dir: &Path) -> io::Result<()> {
    let text = fs::read_to_string(dir.join("app.port"))?;
    let (port, token) = parse_port_file(&text).ok_or(io::ErrorKind::InvalidData)?;
    let address = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(1))?;
    stream.write_all(format!("show {token}\n").as_bytes())
}

/// Whether `a` and `b` are equal, taking as long whatever they hold, so
/// timing tells nothing about the token.
fn same(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0, |diff, (x, y)| diff | (x ^ y)) == 0
}

/// `app.port`: the port, then the token.
fn parse_port_file(text: &str) -> Option<(u16, &str)> {
    let (port, token) = text.trim().split_once(' ')?;
    let port = port.parse().ok()?;
    (!token.is_empty()).then_some((port, token))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_tokens_whole() {
        assert!(same(b"show abc", b"show abc"));
        assert!(!same(b"show abc", b"show abd"));
        assert!(!same(b"show ab", b"show abc"));
        assert!(same(b"", b""));
    }

    /// The folder and the token are this user's only.
    #[test]
    #[cfg(unix)]
    fn keeps_the_token_private() {
        use std::os::unix::fs::PermissionsExt;
        let dir = temp_dir("private");
        let instance = claim(&dir, Duration::ZERO).unwrap().unwrap();
        let mode = |p: &Path| fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&dir), 0o700);
        assert_eq!(mode(&dir.join("app.port")), 0o600);
        drop(instance);
        // Claimed again, as after a crash left the files behind.
        let again = claim(&dir, Duration::ZERO).unwrap().unwrap();
        assert_eq!(mode(&dir.join("app.port")), 0o600);
        drop(again);
        fs::remove_dir_all(dir).unwrap();
    }
    use std::sync::mpsc;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("ferrodock-instance-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn reads_the_port_file() {
        assert_eq!(parse_port_file("4242 abc\n"), Some((4242, "abc")));
        assert_eq!(parse_port_file("4242"), None);
        assert_eq!(parse_port_file("4242 "), None);
        assert_eq!(parse_port_file("port abc"), None);
        assert_eq!(parse_port_file(""), None);
    }

    /// The second Ferrodock finds the first, brings its window forward,
    /// and doesn't run.
    #[test]
    fn a_second_one_wakes_the_first() {
        let dir = temp_dir("wake");
        let first = claim(&dir, Duration::ZERO).unwrap().unwrap();
        let (shown, show) = mpsc::channel();
        first.serve(move || shown.send(()).unwrap()).unwrap();

        assert!(claim(&dir, Duration::ZERO).unwrap().is_none());
        assert!(wake(&dir));
        show.recv_timeout(Duration::from_secs(5)).unwrap();

        // A stranger who doesn't know the token is ignored.
        let port = parse_port_file(&fs::read_to_string(dir.join("app.port")).unwrap())
            .unwrap()
            .0;
        let mut stranger = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap();
        stranger.write_all(b"show nope\n").unwrap();
        drop(stranger);
        assert!(show.recv_timeout(Duration::from_millis(300)).is_err());

        drop(first);
        fs::remove_dir_all(dir).unwrap();
    }

    /// After an update the new version waits for the old one to exit, then
    /// takes over.
    #[test]
    fn waits_for_the_old_one_to_quit() {
        let dir = temp_dir("handoff");
        let old = claim(&dir, Duration::ZERO).unwrap().unwrap();
        let quitting = thread::spawn(move || {
            thread::sleep(Duration::from_millis(300));
            drop(old);
        });
        let started = Instant::now();
        let new = claim(&dir, Duration::from_secs(10)).unwrap();
        assert!(new.is_some());
        assert!(started.elapsed() >= Duration::from_millis(200));
        quitting.join().unwrap();

        // One that never quits is waited for only so long.
        assert!(claim(&dir, Duration::from_millis(200)).unwrap().is_none());
        drop(new);
        assert!(claim(&dir, Duration::ZERO).unwrap().is_some());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn nobody_to_wake() {
        let dir = temp_dir("nobody");
        fs::create_dir_all(&dir).unwrap();
        let started = Instant::now();
        assert!(!wake(&dir));
        assert!(started.elapsed() >= WAKE);
        fs::remove_dir_all(dir).unwrap();
    }
}
