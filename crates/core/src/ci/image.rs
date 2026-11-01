//! The image jobs run in, like GitHub's Ubuntu runners: the toolchains
//! they come with (Rust, Go, Java, Node.js, Python), the usual build tools
//! and CLIs, and a `runner` user with `sudo`. Built here from the
//! Dockerfile below the first time a job needs it.

use super::super::{BuildSpec, DockerClient};

/// Bumped when the Dockerfile changes, so the image is built again.
const REVISION: u32 = 2;
const REPOSITORY: &str = "ferrodock-ci-runner";

/// The Ubuntu version for a job's `runs-on`, or why it can't run here.
pub fn ubuntu_for(runs_on: &[String]) -> Result<&'static str, String> {
    if runs_on.is_empty() {
        return Err("the job has no runs-on".into());
    }
    let labels: Vec<String> = runs_on.iter().map(|l| l.to_lowercase()).collect();
    for label in &labels {
        if label.starts_with("macos") || label.starts_with("windows") {
            return Err(format!(
                "it needs a {} runner, and Local CI runs Linux jobs only",
                if label.starts_with("macos") {
                    "macOS"
                } else {
                    "Windows"
                }
            ));
        }
    }
    Ok(if labels.iter().any(|l| l.starts_with("ubuntu-22.04")) {
        "22.04"
    } else {
        // ubuntu-latest, ubuntu-24.04, self-hosted, linux, …
        "24.04"
    })
}

/// The tag of the runner image for an Ubuntu version.
pub fn tag(ubuntu: &str) -> String {
    format!("{REPOSITORY}:{ubuntu}-{REVISION}")
}

fn dockerfile(ubuntu: &str) -> String {
    format!(
        r#"FROM ubuntu:{ubuntu}
ENV DEBIAN_FRONTEND=noninteractive LANG=C.UTF-8
# The tools GitHub's Ubuntu runners come with, so a workflow behaves the
# same here: build tools, compilers, Python, Java, the usual CLIs.
RUN apt-get update && apt-get install -y --no-install-recommends \
      sudo git git-lfs curl wget ca-certificates gnupg jq zip unzip xz-utils bzip2 zstd \
      p7zip-full tree time parallel less nano file rsync openssh-client \
      dnsutils iputils-ping netcat-openbsd \
      build-essential pkg-config cmake ninja-build clang lld llvm \
      autoconf automake libtool libssl-dev protobuf-compiler sqlite3 libsqlite3-dev \
      python3 python3-pip python3-venv python3-dev pipx \
      openjdk-21-jdk-headless shellcheck \
    && rm -rf /var/lib/apt/lists/* \
    && ln -s "$(dirname $(dirname $(readlink -f $(command -v javac))))" /usr/lib/jvm/java-21
# The GitHub CLI, its latest release.
RUN v=$(curl -fsSL -o /dev/null -w '%{{url_effective}}' https://github.com/cli/cli/releases/latest | sed 's#.*/v##') \
    && curl -fsSL "https://github.com/cli/cli/releases/download/v$v/gh_"$v"_linux_$(dpkg --print-architecture).tar.gz" \
      | tar -xz -C /tmp && mv /tmp/gh_*/bin/gh /usr/local/bin/ && rm -rf /tmp/gh_*
# yq.
RUN curl -fsSL "https://github.com/mikefarah/yq/releases/latest/download/yq_linux_$(dpkg --print-architecture)" \
      -o /usr/local/bin/yq && chmod +x /usr/local/bin/yq
# GitHub's runners run steps as `runner`, with sudo and no password.
RUN (userdel -r ubuntu 2>/dev/null || true) \
    && groupadd -g 1001 runner && useradd -m -u 1001 -g 1001 -s /bin/bash runner \
    && echo 'runner ALL=(ALL) NOPASSWD:ALL' > /etc/sudoers.d/runner \
    && chmod 440 /etc/sudoers.d/runner \
    && mkdir -p /opt/hostedtoolcache /opt/ferrodock-cache /opt/ferrodock-artifacts \
    && chown runner:runner /opt/hostedtoolcache /opt/ferrodock-cache /opt/ferrodock-artifacts
# Node.js LTS, with yarn and pnpm installed, as on GitHub's runners.
RUN arch=$(uname -m | sed 's/x86_64/x64/;s/aarch64/arm64/') \
    && file=$(curl -fsSL https://nodejs.org/dist/latest-v22.x/ | grep -o "node-v22[^\"]*-linux-$arch.tar.xz" | head -1) \
    && curl -fsSL "https://nodejs.org/dist/latest-v22.x/$file" | tar -xJ -C /usr/local --strip-components=1 \
    && npm install -g yarn pnpm && npm cache clean --force
# Go, the latest release.
RUN v=$(curl -fsSL 'https://go.dev/dl/?mode=json' | jq -r '.[0].version') \
    && curl -fsSL "https://go.dev/dl/$v.linux-$(dpkg --print-architecture).tar.gz" | tar -xz -C /usr/local
# Rust, stable with clippy and rustfmt, in the runner's home as on GitHub.
USER runner
RUN curl -fsSL "https://static.rust-lang.org/rustup/dist/$(uname -m)-unknown-linux-gnu/rustup-init" -o /tmp/rustup-init \
    && chmod +x /tmp/rustup-init \
    && /tmp/rustup-init -y --no-modify-path --profile minimal --default-toolchain stable -c clippy -c rustfmt \
    && rm /tmp/rustup-init
USER root
ENV CARGO_HOME=/home/runner/.cargo RUSTUP_HOME=/home/runner/.rustup \
    JAVA_HOME=/usr/lib/jvm/java-21 \
    PATH=/home/runner/.cargo/bin:/usr/local/go/bin:/home/runner/go/bin:/home/runner/.local/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin
LABEL dev.ferrodock.ci.runner="{REVISION}"
"#
    )
}

impl DockerClient {
    /// Makes sure the runner image for `ubuntu` is here, building it if
    /// not. `line` hears the build's output.
    pub async fn ensure_runner_image(
        &self,
        ubuntu: &str,
        mut line: impl FnMut(String),
    ) -> Result<String, String> {
        let tag = tag(ubuntu);
        if self.has_image(&tag).await {
            return Ok(tag);
        }
        line(format!(
            "Building the runner image {tag} (once, a few minutes)…"
        ));
        let dir = std::env::temp_dir().join(format!("ferrodock-ci-runner-{}", std::process::id()));
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        std::fs::write(dir.join("Dockerfile"), dockerfile(ubuntu)).map_err(|e| e.to_string())?;
        let spec = BuildSpec {
            job: 0,
            context: dir.clone(),
            dockerfile: "Dockerfile".into(),
            tag: tag.clone(),
            args: Vec::new(),
            target: String::new(),
            no_cache: false,
        };
        let built = self.build_image(&spec, &mut line).await;
        let _ = std::fs::remove_dir_all(&dir);
        built
            .map(|()| tag)
            .map_err(|e| format!("the runner image couldn't be built: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_the_runner() {
        let labels = |l: &[&str]| l.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        assert_eq!(ubuntu_for(&labels(&["ubuntu-latest"])), Ok("24.04"));
        assert_eq!(ubuntu_for(&labels(&["ubuntu-22.04"])), Ok("22.04"));
        assert_eq!(ubuntu_for(&labels(&["self-hosted", "linux"])), Ok("24.04"));
        assert!(
            ubuntu_for(&labels(&["macos-14"]))
                .unwrap_err()
                .contains("macOS")
        );
        assert!(
            ubuntu_for(&labels(&["windows-latest"]))
                .unwrap_err()
                .contains("Windows")
        );
        assert!(ubuntu_for(&[]).is_err());
        assert!(dockerfile("24.04").starts_with("FROM ubuntu:24.04"));
        assert_eq!(tag("24.04"), "ferrodock-ci-runner:24.04-2");
    }
}
