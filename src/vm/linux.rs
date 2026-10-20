//! The engine on Linux. Docker runs natively here, so Ferrodock's engine
//! is the Docker engine itself, run by systemd as `ferrodock-engine`, with
//! its own socket, data and runtime folders so it never mixes with another
//! engine. It runs as root, so setting it up asks for the user's admin
//! password once (through pkexec, the desktop's own password prompt);
//! after that it starts with the computer, as Docker does on Linux.

use std::path::Path;
use std::process::Command;

use ferrodock_core::builtin::LINUX_SOCKET;

use super::{Paths, Resources, download};

const SERVICE: &str = "ferrodock-engine.service";

/// Run as root with the unpacked programs' folder and the group that may
/// use the engine (the user's own): installs the programs and the
/// service, and starts it.
const SETUP: &str = r#"set -eu
programs=$1 group=$2
install -d -m 0755 /opt/ferrodock/engine/bin
install -m 0755 "$programs"/* /opt/ferrodock/engine/bin/
cat >/etc/systemd/system/ferrodock-engine.service <<EOF
[Unit]
Description=Ferrodock's Docker engine
After=network-online.target firewalld.service
Wants=network-online.target

[Service]
Type=notify
Environment=PATH=/opt/ferrodock/engine/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin
ExecStart=/opt/ferrodock/engine/bin/dockerd --host unix:///run/ferrodock/docker.sock --group $group --data-root /var/lib/ferrodock --exec-root /run/ferrodock/exec --pidfile /run/ferrodock/dockerd.pid
RuntimeDirectory=ferrodock
Delegate=yes
KillMode=process
LimitNOFILE=infinity
LimitNPROC=infinity
TasksMax=infinity
TimeoutStartSec=0
Restart=always
RestartSec=2

[Install]
WantedBy=multi-user.target
EOF
systemctl daemon-reload
systemctl enable --now ferrodock-engine.service
"#;

/// Runs `program` as root through pkexec, which asks for the password.
fn as_root(program: &str, args: &[&str]) -> Result<(), String> {
    let status = Command::new("pkexec")
        .arg(program)
        .args(args)
        .status()
        .map_err(|_| {
            "Ferrodock needs your admin password to set up its engine, but this system \
             has no pkexec (polkit) to ask for it."
                .to_owned()
        })?;
    match status.code() {
        Some(0) => Ok(()),
        // pkexec: the password dialog was dismissed, or not allowed.
        Some(126) | Some(127) => Err(
            "Ferrodock's engine needs your admin password once, to run the Docker engine. \
             Click Set up again to enter it."
                .into(),
        ),
        _ => Err(format!(
            "Setting up the engine failed ({status}). `journalctl -u {SERVICE}` has the details."
        )),
    }
}

/// Unpacks the downloaded programs and installs them as the
/// `ferrodock-engine` service.
pub fn install(paths: &Paths, archive: &[u8]) -> Result<(), String> {
    download::unpack(archive, &paths.image, &["bin/dockerd"])?;
    let group = Command::new("id")
        .arg("-g")
        .output()
        .map_err(|e| e.to_string())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())?;
    let script = paths.dir.join("setup-engine.sh");
    std::fs::write(&script, SETUP).map_err(|e| format!("{}: {e}", script.display()))?;
    let programs = paths.image.join("bin");
    let result = as_root(
        "/bin/sh",
        &[
            &script.to_string_lossy(),
            &programs.to_string_lossy(),
            &group,
        ],
    );
    // The installed copy is the one that runs.
    let _ = std::fs::remove_file(&script);
    let _ = std::fs::remove_dir_all(&paths.image);
    result
}

/// The engine's service. It keeps running when Ferrodock quits.
pub struct Running;

impl Running {
    pub fn start(_paths: &Paths, _resources: &Resources) -> Result<Self, String> {
        let active = Command::new("systemctl")
            .args(["is-active", "--quiet", SERVICE])
            .status()
            .is_ok_and(|s| s.success());
        if !active {
            as_root("systemctl", &["start", SERVICE])?;
        }
        // `is_running` goes by the socket, so be sure it's there.
        for _ in 0..300 {
            if Path::new(LINUX_SOCKET).exists() {
                return Ok(Self);
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        Err(format!(
            "The engine didn't start. `journalctl -u {SERVICE}` has the details."
        ))
    }

    pub fn is_running(&self) -> bool {
        Path::new(LINUX_SOCKET).exists()
    }

    /// Nothing to do: like Docker on Linux, the engine keeps running for
    /// the containers on it.
    pub fn stop(&self) {}
}

#[cfg(test)]
mod tests {
    #[test]
    fn installs_where_the_app_looks() {
        let setup = super::SETUP;
        assert!(setup.contains(&format!("unix://{}", super::LINUX_SOCKET)));
        assert!(setup.contains(ferrodock_core::builtin::LINUX_PROGRAMS));
        assert!(setup.contains(ferrodock_core::builtin::LINUX_SERVICE));
    }
}
