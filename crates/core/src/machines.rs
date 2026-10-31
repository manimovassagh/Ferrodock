//! Linux machines: a container of a Linux distribution kept running to work
//! in, like a small computer. It has your user (with `sudo`), bash and the
//! usual tools, its hostname is its name, and a folder from this computer
//! can be shared into it. Machines carry [`MACHINE_LABEL`].

use std::collections::HashMap;
use std::path::Path;

use bollard::exec::{CreateExecOptions, StartExecResults};
use bollard::models::{ContainerCreateBody, HostConfig};
use bollard::query_parameters::{
    CreateContainerOptionsBuilder, RemoveContainerOptions, StartContainerOptions,
};
use futures_util::StreamExt;

use super::{DockerClient, describe};

/// Marks a machine; the value is its distribution's ID (`ubuntu`).
pub const MACHINE_LABEL: &str = "dev.ferrodock.machine";
/// The folder of this computer shared into a machine, if any.
pub const FOLDER_LABEL: &str = "dev.ferrodock.machine.folder";

/// A Linux distribution a machine can run.
#[derive(Debug, PartialEq, Eq)]
pub struct Distro {
    pub id: &'static str,
    pub name: &'static str,
    pub summary: &'static str,
    /// The image on Docker Hub, without a tag.
    pub image: &'static str,
    /// Versions offered, the default first.
    pub tags: &'static [&'static str],
    /// The badge's colour.
    pub color: [u8; 3],
    /// Installs sudo, bash and the usual tools, as root.
    install: &'static str,
}

/// Red Hat's family. Their small images have `curl-minimal`, which full
/// `curl` replaces.
const DNF: &str = "dnf install -y -q --allowerasing sudo bash curl git nano less procps-ng \
    iputils shadow-utils";

const APT: &str = "export DEBIAN_FRONTEND=noninteractive; apt-get update -q && \
    apt-get install -yq --no-install-recommends sudo bash curl ca-certificates git nano less \
    procps iputils-ping";

pub const DISTROS: &[Distro] = &[
    Distro {
        id: "ubuntu",
        name: "Ubuntu",
        summary: "Popular for servers",
        image: "ubuntu",
        tags: &["24.04", "26.04", "22.04"],
        color: [233, 84, 32],
        install: APT,
    },
    Distro {
        id: "debian",
        name: "Debian",
        summary: "Stable, widely used",
        image: "debian",
        tags: &["13", "12"],
        color: [200, 20, 80],
        install: APT,
    },
    Distro {
        id: "fedora",
        name: "Fedora",
        summary: "The newest packages",
        image: "fedora",
        tags: &["44", "43"],
        color: [60, 110, 180],
        install: "dnf install -y -q sudo bash curl git nano less procps-ng iputils shadow-utils",
    },
    Distro {
        id: "alpine",
        name: "Alpine",
        summary: "Tiny and quick",
        image: "alpine",
        tags: &["3.24", "3.23"],
        color: [13, 89, 127],
        install: "apk add -q --no-cache sudo bash curl git nano less procps shadow",
    },
    Distro {
        id: "arch",
        name: "Arch Linux",
        summary: "Rolling, Intel/AMD only",
        image: "archlinux",
        tags: &["latest"],
        color: [23, 147, 209],
        install: "pacman -Syu --noconfirm --needed sudo bash curl git nano less procps-ng iputils",
    },
    Distro {
        id: "almalinux",
        name: "AlmaLinux",
        summary: "Compatible with Red Hat",
        image: "almalinux",
        tags: &["10", "9"],
        color: [10, 66, 117],
        install: DNF,
    },
    Distro {
        id: "opensuse",
        name: "openSUSE Leap",
        summary: "Stable, from SUSE",
        image: "opensuse/leap",
        tags: &["16.0", "15.6"],
        color: [115, 186, 37],
        install: "zypper --non-interactive --quiet install --no-recommends sudo bash curl git nano \
            less procps iputils shadow",
    },
    Distro {
        id: "amazonlinux",
        name: "Amazon Linux",
        summary: "What AWS servers run",
        image: "amazonlinux",
        tags: &["2023", "2027"],
        color: [236, 114, 17],
        install: DNF,
    },
];

/// The distribution with this ID.
pub fn distro(id: &str) -> Option<&'static Distro> {
    DISTROS.iter().find(|d| d.id == id)
}

/// The user a machine is set up with: your name, and the owner of the
/// shared folder, so files made in it belong to you.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MachineUser {
    pub name: String,
    pub uid: u32,
    pub gid: u32,
}

impl MachineUser {
    /// You, owning `folder` (or your home folder when nothing is shared).
    pub fn current(folder: Option<&Path>) -> Self {
        let login = std::env::var("USER")
            .or_else(|_| std::env::var("USERNAME"))
            .unwrap_or_default();
        let (uid, gid) = owner(folder);
        Self {
            name: user_name(&login),
            uid,
            gid,
        }
    }
}

/// Who owns `folder`, or the home folder. 1000 (the first user on most
/// Linux systems) where that isn't known, and for root.
fn owner(folder: Option<&Path>) -> (u32, u32) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
        let path = folder.map(Path::to_path_buf).or(home);
        if let Some(meta) = path.and_then(|p| std::fs::metadata(p).ok())
            && meta.uid() != 0
        {
            return (meta.uid(), meta.gid());
        }
    }
    let _ = folder;
    (1000, 1000)
}

/// A Linux user name made from a login: `Mani.M` → `mani-m`. `dev` when
/// nothing usable is left.
pub fn user_name(login: &str) -> String {
    let name: String = login
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '-'
            }
        })
        .take(32)
        .collect();
    let name = name.trim_matches('-');
    let usable = name.starts_with(|c: char| c.is_ascii_lowercase() || c == '_');
    if !usable || name == "root" {
        "dev".into()
    } else {
        name.to_owned()
    }
}

/// Everything to create a machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MachineSpec {
    pub distro: &'static Distro,
    pub tag: String,
    /// Also its hostname.
    pub name: String,
    /// A folder of this computer to share into it, if any.
    pub folder: Option<String>,
    /// Removed when it stops.
    pub throwaway: bool,
    pub user: MachineUser,
}

impl MachineSpec {
    pub fn image(&self) -> String {
        format!("{}:{}", self.distro.image, self.tag)
    }

    fn home(&self) -> String {
        format!("/home/{}", self.user.name)
    }

    /// Where the shared folder is in the machine: in your home, under its
    /// own name.
    pub fn mount_point(&self) -> Option<String> {
        let folder = self.folder.as_deref()?;
        let name = Path::new(folder)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| "shared".into());
        Some(format!("{}/{name}", self.home()))
    }

    fn body(&self) -> ContainerCreateBody {
        let mut labels = HashMap::from([(MACHINE_LABEL.to_owned(), self.distro.id.to_owned())]);
        let mut binds = Vec::new();
        if let (Some(folder), Some(mount)) = (&self.folder, self.mount_point()) {
            labels.insert(FOLDER_LABEL.to_owned(), folder.clone());
            binds.push(format!("{folder}:{mount}"));
        }
        ContainerCreateBody {
            image: Some(self.image()),
            hostname: Some(self.name.clone()),
            // By number: the user is only made when the machine is set up.
            user: Some(format!("{}:{}", self.user.uid, self.user.gid)),
            working_dir: Some(self.mount_point().unwrap_or_else(|| self.home())),
            // Keeps it running, whatever the image would start.
            entrypoint: Some(vec!["sleep".into(), "infinity".into()]),
            cmd: Some(Vec::new()),
            env: Some(vec!["LANG=C.UTF-8".into()]),
            labels: Some(labels),
            host_config: Some(HostConfig {
                // Reaps what the shells leave behind.
                init: Some(true),
                auto_remove: Some(self.throwaway),
                binds: (!binds.is_empty()).then_some(binds),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    /// Run as root once the machine has started: the tools, then the user
    /// with your user and group IDs, its home, and `sudo` without a
    /// password. A user the image already has with that ID (Ubuntu's
    /// `ubuntu`) makes way.
    pub fn setup_script(&self) -> String {
        let MachineUser { name, uid, gid } = &self.user;
        let home = self.home();
        format!(
            r#"set -e
( {install} ) || exit {NO_TOOLS}
old=$(getent passwd {uid} | cut -d: -f1 || true)
if [ -n "$old" ] && [ "$old" != {name} ]; then userdel "$old"; fi
getent group {gid} >/dev/null || groupadd -g {gid} {name}
id {name} >/dev/null 2>&1 || useradd -M -u {uid} -g {gid} -d {home} -s /bin/bash {name}
mkdir -p {home}
chown {uid}:{gid} {home}
for f in /etc/skel/.[!.]* /etc/skel/*; do
  if [ -e "$f" ] && [ ! -e "{home}/${{f##*/}}" ]; then
    cp -r "$f" {home}/ && chown -R {uid}:{gid} "{home}/${{f##*/}}"
  fi
done
# Docker starts shells with umask 0, which would make shared files
# writable by anyone.
grep -qs '^umask 022' {home}/.bashrc || echo 'umask 022' >> {home}/.bashrc
chown {uid}:{gid} {home}/.bashrc
mkdir -p /etc/sudoers.d
echo '{name} ALL=(ALL) NOPASSWD:ALL' > /etc/sudoers.d/ferrodock
chmod 440 /etc/sudoers.d/ferrodock
"#,
            install = self.distro.install,
            NO_TOOLS = NO_TOOLS,
        )
    }
}

/// Lines of output kept to explain a failed setup.
const ERROR_LINES: usize = 4;
/// The setup's exit code when the tools couldn't be installed.
const NO_TOOLS: i64 = 3;

impl DockerClient {
    /// Creates and starts the machine's container (its image must be
    /// here). Returns its ID.
    pub async fn create_machine(&self, spec: &MachineSpec) -> Result<String, String> {
        let options = CreateContainerOptionsBuilder::new()
            .name(&spec.name)
            .build();
        let created = self
            .docker
            .create_container(Some(options), spec.body())
            .await
            .map_err(describe)?;
        self.docker
            .start_container(&created.id, None::<StartContainerOptions>)
            .await
            .map_err(describe)?;
        Ok(created.id)
    }

    /// Installs the tools and makes the user. A machine that can't be set
    /// up is removed, so it isn't left half done.
    pub async fn set_up_machine(&self, id: &str, spec: &MachineSpec) -> Result<(), String> {
        let result = self.run_as_root(id, &spec.setup_script()).await;
        if result.is_err() {
            let force = RemoveContainerOptions {
                force: true,
                ..Default::default()
            };
            let _ = self.docker.remove_container(id, Some(force)).await;
        }
        result
    }

    /// Runs `script` with `sh` as root. On failure, the last lines it
    /// printed.
    async fn run_as_root(&self, id: &str, script: &str) -> Result<(), String> {
        let exec = self
            .docker
            .create_exec(
                id,
                CreateExecOptions::<String> {
                    cmd: Some(vec!["sh".into(), "-c".into(), script.into()]),
                    user: Some("root".into()),
                    attach_stdout: Some(true),
                    attach_stderr: Some(true),
                    ..Default::default()
                },
            )
            .await
            .map_err(describe)?;
        let StartExecResults::Attached { mut output, .. } = self
            .docker
            .start_exec(&exec.id, None)
            .await
            .map_err(describe)?
        else {
            return Err("the setup didn't attach".into());
        };
        let mut printed = String::new();
        while let Some(chunk) = output.next().await {
            printed.push_str(&chunk.map_err(describe)?.to_string());
        }
        let code = self
            .docker
            .inspect_exec(&exec.id)
            .await
            .map_err(describe)?
            .exit_code;
        match code {
            Some(0) => return Ok(()),
            Some(NO_TOOLS) => {
                return Err(
                    "it couldn't download sudo, git and the other tools. Check that \
                     containers can reach the internet."
                        .into(),
                );
            }
            _ => {}
        }
        let lines: Vec<&str> = printed.lines().filter(|l| !l.trim().is_empty()).collect();
        let tail = lines[lines.len().saturating_sub(ERROR_LINES)..].join(" · ");
        Err(if tail.is_empty() {
            format!("its setup failed (exit code {})", code.unwrap_or(-1))
        } else {
            format!("its setup failed: {tail}")
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(folder: Option<&str>) -> MachineSpec {
        MachineSpec {
            distro: distro("ubuntu").unwrap(),
            tag: "24.04".into(),
            name: "box".into(),
            folder: folder.map(str::to_owned),
            throwaway: false,
            user: MachineUser {
                name: "mani".into(),
                uid: 501,
                gid: 20,
            },
        }
    }

    #[test]
    fn makes_user_names() {
        assert_eq!(user_name("Mani.M"), "mani-m");
        assert_eq!(user_name("mani_m"), "mani_m");
        assert_eq!(user_name("root"), "dev");
        assert_eq!(user_name("1234"), "dev");
        assert_eq!(user_name(""), "dev");
        assert_eq!(user_name("Ünal"), "nal");
    }

    #[test]
    fn every_distro_is_complete() {
        for d in DISTROS {
            assert!(!d.tags.is_empty(), "{}", d.id);
            assert_eq!(distro(d.id), Some(d));
        }
    }

    #[test]
    fn builds_the_container() {
        let body = spec(None).body();
        assert_eq!(body.image.as_deref(), Some("ubuntu:24.04"));
        assert_eq!(body.hostname.as_deref(), Some("box"));
        assert_eq!(body.user.as_deref(), Some("501:20"));
        assert_eq!(body.working_dir.as_deref(), Some("/home/mani"));
        assert_eq!(body.labels.unwrap()[MACHINE_LABEL], "ubuntu");
        let host = body.host_config.unwrap();
        assert_eq!(host.init, Some(true));
        assert_eq!(host.binds, None);

        let shared = spec(Some("/Users/mani/code/app"));
        assert_eq!(shared.mount_point().as_deref(), Some("/home/mani/app"));
        let body = shared.body();
        assert_eq!(body.working_dir.as_deref(), Some("/home/mani/app"));
        assert_eq!(body.labels.unwrap()[FOLDER_LABEL], "/Users/mani/code/app");
        assert_eq!(
            body.host_config.unwrap().binds.unwrap(),
            ["/Users/mani/code/app:/home/mani/app"]
        );
    }

    /// Talks to a real daemon: `cargo test -- --ignored`. Sets up a
    /// machine of each distribution in `FERRODOCK_MACHINE_DISTROS`
    /// (`alpine` by default; their images must be here, and containers
    /// must reach the distribution's mirrors) with a shared folder, then
    /// checks the user, `sudo` and the folder.
    #[tokio::test]
    #[ignore]
    async fn live_daemon_sets_up_machines() {
        let client = DockerClient::connect().unwrap();
        let ids = std::env::var("FERRODOCK_MACHINE_DISTROS").unwrap_or_else(|_| "alpine".into());
        let shared = std::env::temp_dir().join(format!("ferrodock-machine-{}", std::process::id()));
        std::fs::create_dir_all(&shared).unwrap();
        for id in ids.split(',') {
            let d = distro(id).unwrap();
            let spec = MachineSpec {
                distro: d,
                tag: d.tags[0].into(),
                name: format!("ferrodock-test-{id}"),
                folder: Some(shared.display().to_string()),
                throwaway: false,
                user: MachineUser {
                    name: "tester".into(),
                    uid: 1000,
                    gid: 1000,
                },
            };
            let force = || RemoveContainerOptions {
                force: true,
                ..Default::default()
            };
            let _ = client
                .docker
                .remove_container(&spec.name, Some(force()))
                .await;
            let machine = client.create_machine(&spec).await.unwrap();
            client.set_up_machine(&machine, &spec).await.unwrap();
            let shell = |cmd: &str| {
                let (client, machine, cmd) = (&client, machine.clone(), cmd.to_owned());
                async move {
                    let exec = client
                        .docker
                        .create_exec(
                            &machine,
                            CreateExecOptions::<String> {
                                cmd: Some(vec!["bash".into(), "-lc".into(), cmd]),
                                attach_stdout: Some(true),
                                attach_stderr: Some(true),
                                ..Default::default()
                            },
                        )
                        .await
                        .unwrap();
                    let StartExecResults::Attached { mut output, .. } =
                        client.docker.start_exec(&exec.id, None).await.unwrap()
                    else {
                        panic!("didn't attach");
                    };
                    let mut text = String::new();
                    while let Some(Ok(chunk)) = output.next().await {
                        text.push_str(&chunk.to_string());
                    }
                    text
                }
            };
            let who = shell("whoami; hostname; pwd; echo ~; sudo -n whoami; git --version").await;
            let lines: Vec<&str> = who.lines().collect();
            let mount = spec.mount_point().unwrap();
            assert_eq!(
                lines[..5],
                [
                    "tester",
                    spec.name.as_str(),
                    mount.as_str(),
                    "/home/tester",
                    "root"
                ],
                "{id}: {who}"
            );
            assert!(lines[5].starts_with("git version"), "{id}: {who}");
            // Files made in the shared folder are yours on this computer.
            shell(&format!("touch {mount}/{id}")).await;
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                assert_eq!(std::fs::metadata(shared.join(id)).unwrap().uid(), 1000);
            }
            client
                .docker
                .remove_container(&machine, Some(force()))
                .await
                .unwrap();
        }
        std::fs::remove_dir_all(shared).unwrap();
    }

    #[test]
    fn sets_up_the_user() {
        let script = spec(None).setup_script();
        assert!(script.starts_with("set -e\n"));
        assert!(script.contains("apt-get install"));
        assert!(script.contains("useradd -M -u 501 -g 20 -d /home/mani -s /bin/bash mani"));
        assert!(script.contains("'mani ALL=(ALL) NOPASSWD:ALL'"));
        assert!(script.contains(r#""/home/mani/${f##*/}""#));
        assert!(script.contains("echo 'umask 022' >> /home/mani/.bashrc"));
    }
}
