//! Opening Ferrodock when the user logs in: a LaunchAgent on macOS, the
//! `Run` registry key on Windows, an XDG autostart entry on Linux. The
//! system is the only record of it, so it's read back rather than stored
//! in the settings.

#[cfg(not(windows))]
use std::path::{Path, PathBuf};

/// Passed when started at login: open in the tray, without the window.
pub const BACKGROUND_FLAG: &str = "--background";

/// The command that starts Ferrodock: the app bundle on macOS (so it opens
/// as an app), the executable elsewhere.
fn program() -> Result<Vec<String>, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let bundle = exe
        .ancestors()
        .find(|p| p.extension().is_some_and(|e| e == "app"));
    Ok(match bundle {
        Some(app) if cfg!(target_os = "macos") => vec![
            "/usr/bin/open".into(),
            "-a".into(),
            app.display().to_string(),
            "--args".into(),
            BACKGROUND_FLAG.into(),
        ],
        _ => vec![exe.display().to_string(), BACKGROUND_FLAG.into()],
    })
}

#[cfg(not(windows))]
fn home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

#[cfg(not(windows))]
/// The LaunchAgent that runs `program` at login.
fn launch_agent(program: &[String]) -> String {
    let args: String = program
        .iter()
        .map(|a| format!("        <string>{}</string>\n", xml_escape(a)))
        .collect();
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>dev.ferrodock.app</string>
    <key>ProgramArguments</key>
    <array>
{args}    </array>
    <key>RunAtLoad</key>
    <true/>
</dict>
</plist>
"#
    )
}

#[cfg(not(windows))]
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(not(windows))]
/// The XDG autostart entry that runs `program` at login.
fn desktop_entry(program: &[String]) -> String {
    let exec: Vec<String> = program
        .iter()
        .map(|a| {
            if a.contains(' ') {
                format!("\"{a}\"")
            } else {
                a.clone()
            }
        })
        .collect();
    format!(
        "[Desktop Entry]\nType=Application\nName=Ferrodock\nComment=Docker, at native speed\n\
         Exec={}\nX-GNOME-Autostart-enabled=true\n",
        exec.join(" ")
    )
}

#[cfg(not(windows))]
/// Where the login entry lives (macOS and Linux).
fn entry_path() -> Option<PathBuf> {
    let home = home()?;
    if cfg!(target_os = "macos") {
        Some(home.join("Library/LaunchAgents/dev.ferrodock.app.plist"))
    } else {
        let config = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".config"));
        Some(config.join("autostart/ferrodock.desktop"))
    }
}

#[cfg(windows)]
mod registry {
    use std::os::windows::process::CommandExt;
    use std::process::Command;

    const KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
    /// Keeps `reg` from flashing a console window.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    fn reg(args: &[&str]) -> Result<bool, String> {
        Command::new("reg")
            .args(args)
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .map(|o| o.status.success())
            .map_err(|e| e.to_string())
    }

    pub fn enabled() -> bool {
        reg(&["query", KEY, "/v", "Ferrodock"]).unwrap_or(false)
    }

    pub fn set(command: Option<&str>) -> Result<(), String> {
        let ok = match command {
            Some(command) => reg(&[
                "add",
                KEY,
                "/v",
                "Ferrodock",
                "/t",
                "REG_SZ",
                "/d",
                command,
                "/f",
            ])?,
            None => reg(&["delete", KEY, "/v", "Ferrodock", "/f"])?,
        };
        ok.then_some(())
            .ok_or_else(|| "the registry refused the change".into())
    }
}

/// Whether Ferrodock opens at login.
pub fn enabled() -> bool {
    #[cfg(windows)]
    {
        registry::enabled()
    }
    #[cfg(not(windows))]
    {
        entry_path().is_some_and(|p| p.exists())
    }
}

/// Turns opening at login on or off.
pub fn set(enabled: bool) -> Result<(), String> {
    #[cfg(windows)]
    {
        let command = if enabled {
            let program = program()?;
            Some(format!("\"{}\" {}", program[0], program[1..].join(" ")))
        } else {
            None
        };
        registry::set(command.as_deref())
    }
    #[cfg(not(windows))]
    {
        let path = entry_path().ok_or("no home folder")?;
        if !enabled {
            return match std::fs::remove_file(&path) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.to_string()),
                _ => Ok(()),
            };
        }
        write(&path, &program()?)
    }
}

#[cfg(not(windows))]
fn write(path: &Path, program: &[String]) -> Result<(), String> {
    let text = if cfg!(target_os = "macos") {
        launch_agent(program)
    } else {
        desktop_entry(program)
    };
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, text).map_err(|e| e.to_string())
}

/// Whether this run was started at login.
pub fn started_in_background() -> bool {
    std::env::args().any(|a| a == BACKGROUND_FLAG)
}

#[cfg(all(test, not(windows)))]
mod tests {
    use super::*;

    #[test]
    fn writes_login_entries() {
        let program = vec![
            "/Applications/My Apps/Ferrodock".to_owned(),
            BACKGROUND_FLAG.to_owned(),
        ];
        let plist = launch_agent(&program);
        assert!(plist.contains("<string>/Applications/My Apps/Ferrodock</string>"));
        assert!(plist.contains("<key>RunAtLoad</key>\n    <true/>"));
        let desktop = desktop_entry(&program);
        assert!(desktop.contains("Exec=\"/Applications/My Apps/Ferrodock\" --background\n"));
        assert_eq!(xml_escape("a&b<c>"), "a&amp;b&lt;c&gt;");
    }

    /// Writes and removes the real login entry (XDG on Linux) under a
    /// temporary config folder.
    #[test]
    #[cfg(target_os = "linux")]
    fn turns_on_and_off() {
        let dir = std::env::temp_dir().join(format!("ferrodock-autostart-{}", std::process::id()));
        // Only this test touches XDG_CONFIG_HOME.
        unsafe { std::env::set_var("XDG_CONFIG_HOME", &dir) };
        set(true).unwrap();
        assert!(enabled());
        let entry = std::fs::read_to_string(dir.join("autostart/ferrodock.desktop")).unwrap();
        assert!(entry.contains(BACKGROUND_FLAG));
        set(false).unwrap();
        assert!(!enabled());
        set(false).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
