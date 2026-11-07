//! Restarting into an update, and going back when it won't start.
//!
//! The old version's binary, kept in the update's working folder, stays
//! behind as a watcher (`--update-watch`). It waits for the old app to
//! quit, so the two never run at once, then starts the new one. If the new
//! one exits before it has run for [`SETTLE`], it crashed: the watcher puts
//! the old version back, starts it, and leaves a note so the update isn't
//! offered again.

use std::fs::{self, File, TryLockError};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use super::install::Installed;

/// Runs this binary as the watcher of an update.
pub const WATCH_FLAG: &str = "--update-watch";

/// How long the new version runs before it counts as started. Long
/// enough for a crash shortly after launch, too.
pub const SETTLE: Duration = Duration::from_secs(30);

/// How long the watcher waits for the old version to quit, and for the
/// new one to take over.
const WAIT: Duration = Duration::from_secs(150);

/// The watcher gives up watching after this long, rolling nothing back.
const GIVE_UP: Duration = Duration::from_secs(600);

/// Written into the working folder by a new version that started.
const STARTED: &str = "started";

/// Left in Ferrodock's folder by a rollback: the version that didn't
/// start, so it isn't offered again.
const SKIPPED: &str = "update-skipped";

/// Left with [`SKIPPED`], and read once, to say what happened.
const ROLLED_BACK: &str = "update-rolled-back";

/// The watcher's arguments: what was installed, where its working folder
/// is, and which version it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Watch {
    pub installed: Installed,
    pub work: PathBuf,
    pub version: String,
}

impl Watch {
    fn args(&self) -> Vec<String> {
        let (kind, path) = match &self.installed {
            Installed::App(path) => ("app", path),
            Installed::Exe(path) => ("exe", path),
        };
        vec![
            WATCH_FLAG.into(),
            kind.into(),
            path.display().to_string(),
            self.work.display().to_string(),
            self.version.clone(),
        ]
    }

    fn parse(args: &[String]) -> Option<Self> {
        let at = args.iter().position(|a| a == WATCH_FLAG)?;
        let [kind, path, work, version] = args.get(at + 1..at + 5)? else {
            return None;
        };
        let path = PathBuf::from(path);
        let installed = match kind.as_str() {
            "app" => Installed::App(path),
            "exe" => Installed::Exe(path),
            _ => return None,
        };
        Some(Self {
            installed,
            work: work.into(),
            version: version.clone(),
        })
    }

    /// The old version, as kept in the working folder.
    fn previous(&self, exe_name: &str) -> PathBuf {
        match &self.installed {
            Installed::App(_) => self.work.join("previous.app"),
            Installed::Exe(_) => self.work.join("previous").join(exe_name),
        }
    }

    /// Starts the watcher from the old version's binary. The caller then
    /// quits.
    pub fn spawn(&self) -> std::io::Result<()> {
        let exe = std::env::current_exe()?;
        let name = exe.file_name().unwrap_or_default().to_string_lossy();
        let program = match &self.installed {
            Installed::App(_) => self
                .previous(&name)
                .join("Contents/MacOS")
                .join(name.as_ref()),
            Installed::Exe(_) => self.previous(&name),
        };
        std::process::Command::new(program)
            .args(self.args())
            .spawn()
            .map(drop)
    }
}

/// When this process is a watcher, watches and returns true: it should
/// then exit.
pub fn run_if_asked() -> bool {
    let args: Vec<String> = std::env::args().collect();
    let Some(watch) = Watch::parse(&args) else {
        return false;
    };
    let Some(dir) = crate::vm::Paths::locate().map(|p| p.dir) else {
        // No lock to go by: start the new version and leave it.
        let _ = watch.installed.launch();
        return true;
    };
    let lock = dir.join("app.lock");
    let exe_name = std::env::current_exe()
        .ok()
        .and_then(|e| e.file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_default();
    if outcome(&watch, &lock, &|| watch.installed.launch().is_ok()) == Outcome::Failed
        && roll_back(&watch, &exe_name).is_ok()
    {
        note(&dir, &watch.version);
        let _ = watch.installed.launch();
    }
    true
}

#[derive(Debug, PartialEq, Eq)]
enum Outcome {
    Started,
    Failed,
    /// Neither, after [`GIVE_UP`]: left alone.
    Unknown,
}

/// Waits for the old version to quit, starts the new one with `launch`,
/// and tells whether it started.
fn outcome(watch: &Watch, lock: &Path, launch: &dyn Fn() -> bool) -> Outcome {
    let begun = Instant::now();
    // The old version holds the lock until it has stopped its engine.
    while held(lock) && begun.elapsed() < WAIT {
        thread::sleep(Duration::from_millis(200));
    }
    if !launch() {
        return Outcome::Failed;
    }
    let launched = Instant::now();
    let started = watch.work.join(STARTED);
    let mut seen = false;
    loop {
        if started.exists() {
            return Outcome::Started;
        }
        let running = held(lock);
        seen |= running;
        // It took the lock and let go without saying it started: it
        // crashed. Or it never took it at all.
        if (seen && !running) || (!seen && launched.elapsed() > WAIT) {
            // A clean quit writes `started` just before letting go.
            thread::sleep(Duration::from_millis(500));
            return if started.exists() {
                Outcome::Started
            } else {
                Outcome::Failed
            };
        }
        if launched.elapsed() > GIVE_UP {
            return Outcome::Unknown;
        }
        thread::sleep(Duration::from_millis(500));
    }
}

/// Whether a Ferrodock holds the lock. Taken for a moment if it's free:
/// a Ferrodock starting after an update waits for it, so that's harmless.
fn held(lock: &Path) -> bool {
    let Ok(file) = File::options()
        .create(true)
        .truncate(false)
        .write(true)
        .open(lock)
    else {
        return false;
    };
    matches!(file.try_lock(), Err(TryLockError::WouldBlock))
}

/// Puts the old version back in place of the new one.
fn roll_back(watch: &Watch, exe_name: &str) -> std::io::Result<()> {
    let previous = watch.previous(exe_name);
    match &watch.installed {
        Installed::App(app) => {
            let failed = watch.work.join("failed.app");
            let _ = fs::remove_dir_all(&failed);
            fs::rename(app, &failed)?;
            if let Err(e) = fs::rename(&previous, app) {
                let _ = fs::rename(&failed, app);
                return Err(e);
            }
        }
        Installed::Exe(exe) => {
            let failed = watch.work.join("failed");
            let _ = fs::remove_file(&failed);
            fs::rename(exe, &failed)?;
            // Copied, not moved: it's this watcher's own binary.
            if let Err(e) = fs::copy(&previous, exe) {
                let _ = fs::rename(&failed, exe);
                return Err(e);
            }
        }
    }
    Ok(())
}

fn note(dir: &Path, version: &str) {
    let _ = fs::write(dir.join(SKIPPED), version);
    let _ = fs::write(dir.join(ROLLED_BACK), version);
}

/// Called by a new version: it started, so the old one is no longer
/// needed. Marks the update folders next to it, but not one this process
/// is making: quitting to restart into an update isn't a start.
pub fn mark_started() {
    let own = format!(".ferrodock-update-{}", std::process::id());
    for work in super::install::work_dirs() {
        if work.file_name().is_some_and(|n| n.to_string_lossy() != own) {
            let _ = fs::write(work.join(STARTED), "");
        }
    }
}

/// The version a rollback skipped, if any.
pub fn skipped() -> Option<String> {
    let dir = crate::vm::Paths::locate()?.dir;
    let version = fs::read_to_string(dir.join(SKIPPED)).ok()?;
    Some(version.trim().to_owned()).filter(|v| !v.is_empty())
}

/// The version that didn't start, once, after a rollback.
pub fn rolled_back() -> Option<String> {
    let dir = crate::vm::Paths::locate()?.dir;
    let file = dir.join(ROLLED_BACK);
    let version = fs::read_to_string(&file).ok()?;
    let _ = fs::remove_file(file);
    Some(version.trim().to_owned()).filter(|v| !v.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ferrodock-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn passes_its_arguments() {
        let watch = Watch {
            installed: Installed::App("/Applications/Ferrodock.app".into()),
            work: "/Applications/.ferrodock-update-7".into(),
            version: "2.0.0".into(),
        };
        let mut args = vec!["ferrodock".to_owned()];
        args.extend(watch.args());
        assert_eq!(Watch::parse(&args), Some(watch));
        assert_eq!(Watch::parse(&["ferrodock".into()]), None);
        assert_eq!(
            Watch::parse(&["x".into(), WATCH_FLAG.into(), "app".into()]),
            None
        );
    }

    #[test]
    fn rolls_back_a_binary() {
        let dir = scratch("roll-back-exe");
        let exe = dir.join("ferrodock");
        fs::write(&exe, "new").unwrap();
        let work = dir.join(".ferrodock-update-1");
        fs::create_dir_all(work.join("previous")).unwrap();
        fs::write(work.join("previous/ferrodock"), "old").unwrap();
        let watch = Watch {
            installed: Installed::Exe(exe.clone()),
            work: work.clone(),
            version: "2.0.0".into(),
        };
        roll_back(&watch, "ferrodock").unwrap();
        assert_eq!(fs::read_to_string(&exe).unwrap(), "old");
        assert_eq!(fs::read_to_string(work.join("failed")).unwrap(), "new");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn rolls_back_an_app() {
        let dir = scratch("roll-back-app");
        let app = dir.join("Ferrodock.app");
        fs::create_dir_all(&app).unwrap();
        fs::write(app.join("v"), "new").unwrap();
        let work = dir.join(".ferrodock-update-1");
        fs::create_dir_all(work.join("previous.app")).unwrap();
        fs::write(work.join("previous.app/v"), "old").unwrap();
        let watch = Watch {
            installed: Installed::App(app.clone()),
            work: work.clone(),
            version: "2.0.0".into(),
        };
        roll_back(&watch, "ferrodock").unwrap();
        assert_eq!(fs::read_to_string(app.join("v")).unwrap(), "old");
        assert_eq!(
            fs::read_to_string(work.join("failed.app/v")).unwrap(),
            "new"
        );
        fs::remove_dir_all(dir).unwrap();
    }

    /// Plays the new version in a thread: takes the lock, then either says
    /// it started or lets go without saying so.
    fn new_version(lock: PathBuf, work: PathBuf, starts: bool) -> impl Fn() -> bool {
        move || {
            let (lock, work) = (lock.clone(), work.clone());
            thread::spawn(move || {
                let file = File::options().write(true).open(&lock).unwrap();
                file.lock().unwrap();
                // Longer than the watcher's poll, so it sees it run.
                thread::sleep(Duration::from_millis(1500));
                if starts {
                    fs::write(work.join(STARTED), "").unwrap();
                }
                drop(file);
            });
            true
        }
    }

    #[test]
    fn tells_a_start_from_a_crash() {
        for starts in [true, false] {
            let dir = scratch(&format!("watch-{starts}"));
            let lock = dir.join("app.lock");
            fs::write(&lock, "").unwrap();
            let work = dir.join("work");
            fs::create_dir_all(&work).unwrap();
            let watch = Watch {
                installed: Installed::Exe(dir.join("ferrodock")),
                work: work.clone(),
                version: "2.0.0".into(),
            };
            let launch = new_version(lock.clone(), work, starts);
            let expected = if starts {
                Outcome::Started
            } else {
                Outcome::Failed
            };
            assert_eq!(outcome(&watch, &lock, &launch), expected);
            fs::remove_dir_all(dir).unwrap();
        }
    }

    #[test]
    fn waits_for_the_old_version_to_quit() {
        let dir = scratch("watch-handoff");
        let lock = dir.join("app.lock");
        fs::write(&lock, "").unwrap();
        let work = dir.join("work");
        fs::create_dir_all(&work).unwrap();
        // The old version, still quitting.
        let old = File::options().write(true).open(&lock).unwrap();
        old.lock().unwrap();
        let quit = Instant::now();
        let releaser = thread::spawn(move || {
            thread::sleep(Duration::from_millis(400));
            drop(old);
        });
        let watch = Watch {
            installed: Installed::Exe(dir.join("ferrodock")),
            work: work.clone(),
            version: "2.0.0".into(),
        };
        let launched_after = std::sync::Mutex::new(None);
        let inner = new_version(lock.clone(), work, true);
        let launch = || {
            *launched_after.lock().unwrap() = Some(quit.elapsed());
            inner()
        };
        assert_eq!(outcome(&watch, &lock, &launch), Outcome::Started);
        assert!(launched_after.lock().unwrap().unwrap() >= Duration::from_millis(400));
        releaser.join().unwrap();
        fs::remove_dir_all(dir).unwrap();
    }
}
