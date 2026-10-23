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

/// Run as root, given inline (never as a file the user could change
/// first): installs the programs and the service, and starts it.
///
/// The engine's socket belongs to its own `ferrodock` group, which the
/// user joins, rather than the user's primary group: that's shared on some
/// systems (`users` on openSUSE, many LDAP setups), and access to the
/// socket is root access.
///
/// Arguments: the unpacked programs' folder, the user who may use the
/// engine, then `name=sha256` for each program. That folder is the user's
/// own, so each program is copied somewhere only root can write and
/// checked there: one swapped after Ferrodock checked it (by anything else
/// running as the user, between the download and the password prompt)
/// isn't installed.
const SETUP: &str = r#"set -eu
programs=$1 user=$2
shift 2
getent group ferrodock >/dev/null || groupadd --system ferrodock
usermod -a -G ferrodock "$user"
install -d -m 0755 /opt/ferrodock /opt/ferrodock/engine/bin
stage=$(mktemp -d /opt/ferrodock/.setup.XXXXXX)
trap 'rm -rf "$stage"' EXIT
for entry in "$@"; do
  name=${entry%%=*} sum=${entry#*=}
  case $name in ''|.*|*/*) echo "Unexpected program name: $name" >&2; exit 1;; esac
  cp -- "$programs/$name" "$stage/$name"
  if ! echo "$sum  $stage/$name" | sha256sum -c --quiet --status -; then
    echo "$name changed after it was downloaded; not installing it." >&2
    exit 1
  fi
done
install -m 0755 "$stage"/* /opt/ferrodock/engine/bin/
cat >/etc/systemd/system/ferrodock-engine.service <<EOF
[Unit]
Description=Ferrodock's Docker engine
After=network-online.target firewalld.service
Wants=network-online.target

[Service]
Type=notify
Environment=PATH=/opt/ferrodock/engine/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin
ExecStart=/opt/ferrodock/engine/bin/dockerd --host unix:///run/ferrodock/docker.sock --group ferrodock --data-root /var/lib/ferrodock --exec-root /run/ferrodock/exec --pidfile /run/ferrodock/dockerd.pid
# Joining a group only counts from the next login; owning the socket lets
# the user in straight away (it's root:ferrodock, 0660, otherwise).
ExecStartPost=/bin/chown $user /run/ferrodock/docker.sock
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
systemctl enable ferrodock-engine.service
# Restarted, not just started: an engine set up before ran with the
# user's primary group, and so would keep running until the next boot.
systemctl restart ferrodock-engine.service
"#;

/// Run as root: stops the service, deletes its data (images, containers,
/// volumes), and starts it again, empty. The data folder is the
/// `--data-root` in [`SETUP`]; folders mounted inside it are left alone.
const RESET: &str = "set -eu
systemctl stop ferrodock-engine.service
rm -rf --one-file-system /var/lib/ferrodock
systemctl start ferrodock-engine.service
";

/// Runs `program` as root through pkexec, which asks for the password.
/// `doing` says what for, in errors: "Setting up the engine".
fn as_root(doing: &str, program: &str, args: &[&str]) -> Result<(), String> {
    let status = Command::new("pkexec")
        .arg(program)
        .args(args)
        .status()
        .map_err(|_| {
            format!(
                "{doing} needs your admin password, but this system has no pkexec (polkit) \
                 to ask for it."
            )
        })?;
    match status.code() {
        Some(0) => Ok(()),
        // pkexec: the password dialog was dismissed, or not allowed.
        Some(126) | Some(127) => Err(format!(
            "{doing} needs your admin password: the Docker engine runs as root. \
             Try again to enter it."
        )),
        _ => Err(format!(
            "{doing} failed ({status}). `journalctl -u {SERVICE}` has the details."
        )),
    }
}

/// Unpacks the downloaded programs and installs them as the
/// `ferrodock-engine` service, for this user.
pub fn install(paths: &Paths, archive: &[u8]) -> Result<(), String> {
    download::unpack(archive, &paths.image, &["bin/dockerd"])?;
    let user = Command::new("id")
        .arg("-un")
        .output()
        .map_err(|e| e.to_string())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())?;
    let programs = paths.image.join("bin");
    let result = checksums(&programs).and_then(|sums| {
        let mut args = vec![
            "-c".to_owned(),
            SETUP.to_owned(),
            // $0 for the script, then its arguments.
            "ferrodock-setup".to_owned(),
            programs.to_string_lossy().into_owned(),
            user,
        ];
        args.extend(sums);
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        as_root("Setting up the engine", "/bin/sh", &args)
    });
    // The installed copy is the one that runs.
    let _ = std::fs::remove_dir_all(&paths.image);
    result
}

/// `name=sha256` for each program in `dir`, for [`SETUP`] to check its
/// copies against.
fn checksums(dir: &Path) -> Result<Vec<String>, String> {
    use sha2::{Digest, Sha256};
    let mut sums = Vec::new();
    for entry in std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let bytes = std::fs::read(entry.path()).map_err(|e| format!("{name}: {e}"))?;
        sums.push(format!(
            "{name}={}",
            ferrodock_core::hex(&Sha256::digest(&bytes))
        ));
    }
    sums.sort();
    Ok(sums)
}

/// Stops the service, deletes its data and starts it again.
pub fn reset(_paths: &Paths) -> Result<(), String> {
    as_root("Resetting the engine", "/bin/sh", &["-c", RESET])
}

/// Installs a newer image's programs (asking for the password again).
/// Images, containers and volumes are in `/var/lib/ferrodock`, apart from
/// them, so they stay. The service starts with the new programs; one that
/// runs already keeps the old ones until it restarts (with the computer),
/// so its containers aren't stopped.
pub fn update(paths: &Paths, archive: &[u8]) -> Result<(), String> {
    install(paths, archive)
}

/// The engine's service. It keeps running when Ferrodock quits.
pub struct Running;

impl Running {
    /// The service, when it runs already (it starts with the computer).
    pub fn find() -> Option<Self> {
        Path::new(LINUX_SOCKET).exists().then_some(Self)
    }

    pub fn start(_paths: &Paths, _resources: &Resources) -> Result<Self, String> {
        let active = Command::new("systemctl")
            .args(["is-active", "--quiet", SERVICE])
            .status()
            .is_ok_and(|s| s.success());
        if !active {
            as_root("Starting the engine", "systemctl", &["start", SERVICE])?;
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

    /// Stopped from Settings: the service stays down until it's started
    /// again, or the computer restarts.
    pub fn shut_down(&self) -> Result<(), String> {
        as_root("Stopping the engine", "systemctl", &["stop", SERVICE])
    }
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

    #[test]
    fn only_its_own_group_may_use_the_engine() {
        let setup = super::SETUP;
        assert!(setup.contains("--group ferrodock "));
        assert!(setup.contains("groupadd --system ferrodock"));
        assert!(setup.contains("usermod -a -G ferrodock \"$user\""));
        assert!(!setup.contains("$group"));
        assert!(setup.contains("ExecStartPost=/bin/chown $user /run/ferrodock/docker.sock"));
    }

    #[test]
    fn checksums_every_program() {
        let dir = std::env::temp_dir().join(format!("ferrodock-sums-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("dockerd"), b"abc").unwrap();
        std::fs::write(dir.join("containerd"), b"").unwrap();
        let sums = super::checksums(&dir).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(
            sums,
            [
                "containerd=e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
                "dockerd=ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
            ]
        );
    }

    #[test]
    fn resets_the_data_the_service_keeps() {
        assert!(super::SETUP.contains("--data-root /var/lib/ferrodock "));
        assert!(super::RESET.contains("rm -rf --one-file-system /var/lib/ferrodock\n"));
        assert!(super::RESET.contains("start ferrodock-engine.service"));
    }
}
