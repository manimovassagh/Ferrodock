//! Puts the command-line programs that come with Ferrodock (`fdock` and
//! `ferrodock-mcp`) on the PATH, so they work in any new terminal:
//!
//! - macOS: links in /usr/local/bin (asking for an administrator password
//!   when that folder needs it);
//! - Linux: links in ~/.local/bin;
//! - Windows: adds Ferrodock's folder to the user's Path.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// The programs, by name (without `.exe`).
pub const PROGRAMS: [&str; 2] = ["fdock", "ferrodock-mcp"];

fn file_name(program: &str) -> String {
    if cfg!(windows) {
        format!("{program}.exe")
    } else {
        program.to_owned()
    }
}

/// The folder Ferrodock runs from, when the programs are in it.
pub fn bundled_dir() -> Option<PathBuf> {
    let dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    PROGRAMS
        .iter()
        .all(|p| dir.join(file_name(p)).is_file())
        .then_some(dir)
}

/// Where the links go (macOS and Linux).
#[cfg(not(windows))]
fn link_dir() -> Option<PathBuf> {
    if cfg!(target_os = "macos") {
        Some(PathBuf::from("/usr/local/bin"))
    } else {
        std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/bin"))
    }
}

/// Folders where a terminal finds commands. On macOS an app opened from
/// Finder has a short PATH, so the usual folders are added.
fn search_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect())
        .unwrap_or_default();
    if cfg!(target_os = "macos") {
        dirs.extend(["/usr/local/bin", "/opt/homebrew/bin"].map(PathBuf::from));
    }
    #[cfg(not(windows))]
    dirs.extend(link_dir());
    #[cfg(windows)]
    dirs.extend(windows::user_path().unwrap_or_default());
    dirs
}

/// Where `program` is found from a new terminal, if it is.
pub fn installed(program: &str) -> Option<PathBuf> {
    let file = file_name(program);
    search_dirs()
        .into_iter()
        .map(|dir| dir.join(&file))
        .find(|path| path.is_file())
}

/// Whether every program is on the PATH.
pub fn all_installed() -> bool {
    PROGRAMS.iter().all(|p| installed(p).is_some())
}

/// Whether installing needs an administrator password.
pub fn needs_password() -> bool {
    #[cfg(not(windows))]
    {
        cfg!(target_os = "macos") && !link_dir().is_some_and(|dir| writable(&dir))
    }
    #[cfg(windows)]
    {
        false
    }
}

#[cfg(not(windows))]
fn writable(dir: &Path) -> bool {
    let probe = dir.join(format!(".ferrodock-probe-{}", std::process::id()));
    let ok = std::fs::write(&probe, b"").is_ok();
    let _ = std::fs::remove_file(&probe);
    ok
}

/// Puts the programs from `dir` on the PATH.
pub fn install(dir: &Path) -> Result<(), String> {
    #[cfg(not(windows))]
    {
        let target = link_dir().ok_or("no home folder")?;
        if std::fs::create_dir_all(&target).is_ok() && writable(&target) {
            for program in PROGRAMS {
                let link = target.join(program);
                let _ = std::fs::remove_file(&link);
                std::os::unix::fs::symlink(dir.join(program), &link)
                    .map_err(|e| format!("couldn't link {}: {e}", link.display()))?;
            }
            Ok(())
        } else if cfg!(target_os = "macos") {
            as_administrator(dir, &target)
        } else {
            Err(format!("can't write to {}", target.display()))
        }
    }
    #[cfg(windows)]
    {
        windows::add_to_user_path(dir)
    }
}

/// Makes the links with an administrator password, through the system's
/// own password dialog.
#[cfg(target_os = "macos")]
fn as_administrator(dir: &Path, target: &Path) -> Result<(), String> {
    let quote = |p: &Path| format!("'{}'", p.display().to_string().replace('\'', r"'\''"));
    let mut script = format!("mkdir -p {}", quote(target));
    for program in PROGRAMS {
        script += &format!(
            " && ln -sf {} {}",
            quote(&dir.join(program)),
            quote(&target.join(program))
        );
    }
    let applescript = format!(
        "do shell script \"{}\" with prompt \"Ferrodock wants to install the fdock command.\" \
         with administrator privileges",
        script.replace('\\', "\\\\").replace('"', "\\\"")
    );
    let output = std::process::Command::new("/usr/bin/osascript")
        .args(["-e", &applescript])
        .output()
        .map_err(|e| e.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        let message = String::from_utf8_lossy(&output.stderr);
        Err(if message.contains("-128") {
            "cancelled".into()
        } else {
            message.trim().to_owned()
        })
    }
}

#[cfg(all(not(windows), not(target_os = "macos")))]
fn as_administrator(_dir: &Path, _target: &Path) -> Result<(), String> {
    Err("not supported".into())
}

/// Where an install started from the app has got to.
#[derive(Clone, PartialEq)]
pub enum Progress {
    Idle,
    Running,
    Failed(String),
}

static PROGRESS: Mutex<Progress> = Mutex::new(Progress::Idle);

/// Where each of [`PROGRAMS`] is found from a new terminal.
#[derive(Clone, Default)]
pub struct Found([Option<PathBuf>; PROGRAMS.len()]);

impl Found {
    /// Whether every program is on the PATH.
    pub fn installed(&self) -> bool {
        self.0.iter().all(Option::is_some)
    }

    pub fn path(&self, program: &str) -> Option<&PathBuf> {
        let i = PROGRAMS.iter().position(|p| *p == program)?;
        self.0[i].as_ref()
    }
}

enum Lookup {
    Unknown,
    Looking,
    Done(Found),
}

/// The last [`Found`]. Looking takes a moment on Windows (it asks
/// PowerShell for the user's Path), so it's done once, off the UI thread,
/// and again after an install.
static FOUND: Mutex<Lookup> = Mutex::new(Lookup::Unknown);

pub fn progress() -> Progress {
    PROGRESS.lock().map(|p| p.clone()).unwrap_or(Progress::Idle)
}

/// Where the programs are, once known. The first call starts looking on
/// a background thread and returns None; `repaint` runs when it's done.
pub fn found(repaint: impl FnOnce() + Send + 'static) -> Option<Found> {
    let mut lookup = FOUND.lock().ok()?;
    match &*lookup {
        Lookup::Done(found) => return Some(found.clone()),
        Lookup::Looking => return None,
        Lookup::Unknown => *lookup = Lookup::Looking,
    }
    std::thread::spawn(move || {
        let found = Found(PROGRAMS.map(installed));
        if let Ok(mut lookup) = FOUND.lock() {
            *lookup = Lookup::Done(found);
        }
        repaint();
    });
    None
}

/// Installs in the background; `done` runs when it's finished.
pub fn start(dir: PathBuf, done: impl FnOnce() + Send + 'static) {
    if let Ok(mut progress) = PROGRESS.lock() {
        if *progress == Progress::Running {
            return;
        }
        *progress = Progress::Running;
    }
    std::thread::spawn(move || {
        let result = install(&dir);
        if let Ok(mut lookup) = FOUND.lock() {
            *lookup = Lookup::Unknown;
        }
        if let Ok(mut progress) = PROGRESS.lock() {
            *progress = match result {
                Ok(()) => Progress::Idle,
                Err(e) => Progress::Failed(e),
            };
        }
        done();
    });
}

/// Commands that do the same by hand, for this system.
pub fn manual_steps(dir: &Path) -> Vec<String> {
    let shown = dir.display().to_string();
    if cfg!(windows) {
        vec![format!(
            "[Environment]::SetEnvironmentVariable('Path', \
             [Environment]::GetEnvironmentVariable('Path', 'User') + ';{shown}', 'User')"
        )]
    } else if cfg!(target_os = "macos") {
        PROGRAMS
            .iter()
            .map(|p| format!("sudo ln -sf \"{shown}/{p}\" /usr/local/bin/{p}"))
            .collect()
    } else {
        let mut steps = vec!["mkdir -p ~/.local/bin".to_owned()];
        steps.extend(
            PROGRAMS
                .iter()
                .map(|p| format!("ln -sf \"{shown}/{p}\" ~/.local/bin/{p}")),
        );
        steps
    }
}

#[cfg(windows)]
mod windows {
    use std::os::windows::process::CommandExt;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    /// Keeps PowerShell from flashing a console window.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    fn powershell(script: &str, dir: Option<&Path>) -> Result<String, String> {
        let mut command = Command::new("powershell");
        command
            .args(["-NoProfile", "-NonInteractive", "-Command", script])
            .creation_flags(CREATE_NO_WINDOW);
        if let Some(dir) = dir {
            command.env("FERRODOCK_DIR", dir);
        }
        let output = command.output().map_err(|e| e.to_string())?;
        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).into_owned())
        } else {
            Err(String::from_utf8_lossy(&output.stderr).trim().to_owned())
        }
    }

    /// The user's own Path, as saved (new terminals get it; this process
    /// still has the one from when it started).
    pub fn user_path() -> Option<Vec<PathBuf>> {
        let out = powershell(
            "[Environment]::GetEnvironmentVariable('Path', 'User')",
            None,
        )
        .ok()?;
        Some(
            out.trim()
                .split(';')
                .filter(|p| !p.is_empty())
                .map(PathBuf::from)
                .collect(),
        )
    }

    /// Adds `dir` to the user's Path (kept unexpanded, as Windows saves
    /// it), then tells open programs that it changed.
    pub fn add_to_user_path(dir: &Path) -> Result<(), String> {
        const SCRIPT: &str = r#"
$dir = $env:FERRODOCK_DIR
$path = (Get-Item 'HKCU:\Environment').GetValue('Path', '', 'DoNotExpandEnvironmentNames')
$parts = @($path -split ';' | Where-Object { $_ -ne '' })
if ($parts -notcontains $dir) {
  $new = (@($parts) + $dir) -join ';'
  New-ItemProperty -Path 'HKCU:\Environment' -Name Path -Value $new -PropertyType ExpandString -Force | Out-Null
  [Environment]::SetEnvironmentVariable('FERRODOCK_PATH_CHANGED', '1', 'User')
  [Environment]::SetEnvironmentVariable('FERRODOCK_PATH_CHANGED', $null, 'User')
}
"#;
        powershell(SCRIPT, Some(dir)).map(|_| ())
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn manual_steps_name_both_programs() {
        let steps =
            manual_steps(Path::new("/Applications/Ferrodock.app/Contents/MacOS")).join("\n");
        for program in PROGRAMS {
            assert!(steps.contains(program), "{steps}");
        }
    }
}

/// Installs for real into a scratch home folder. Changes HOME, so run it
/// on its own: `cargo test links_into_local_bin -- --ignored`.
#[cfg(all(test, target_os = "linux"))]
#[test]
#[ignore]
fn links_into_local_bin() {
    let root = std::env::temp_dir().join(format!("ferrodock-cli-{}", std::process::id()));
    let app = root.join("app");
    std::fs::create_dir_all(&app).unwrap();
    for program in PROGRAMS {
        std::fs::write(app.join(program), "").unwrap();
    }
    // SAFETY: this test runs on its own (`--ignored`).
    unsafe { std::env::set_var("HOME", root.join("home")) };
    install(&app).unwrap();
    install(&app).unwrap();
    for program in PROGRAMS {
        let link = root.join("home/.local/bin").join(program);
        assert_eq!(std::fs::read_link(&link).unwrap(), app.join(program));
        assert!(installed(program).is_some());
    }
    std::fs::remove_dir_all(&root).unwrap();
}
