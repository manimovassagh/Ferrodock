//! The image jobs run in, like GitHub's Ubuntu runners but much smaller:
//! the usual tools, Node, Python, and a `runner` user with `sudo`. Built
//! here from the Dockerfile below the first time a job needs it.

use super::super::{BuildSpec, DockerClient};

/// Bumped when the Dockerfile changes, so the image is built again.
const REVISION: u32 = 1;
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
RUN apt-get update && apt-get install -y --no-install-recommends \
      sudo git curl wget ca-certificates gnupg jq zip unzip xz-utils bzip2 \
      build-essential pkg-config python3 python3-pip python3-venv \
      openssh-client rsync less nano file \
    && rm -rf /var/lib/apt/lists/*
# GitHub's runners run steps as `runner`, with sudo and no password.
RUN (userdel -r ubuntu 2>/dev/null || true) \
    && groupadd -g 1001 runner && useradd -m -u 1001 -g 1001 -s /bin/bash runner \
    && echo 'runner ALL=(ALL) NOPASSWD:ALL' > /etc/sudoers.d/runner \
    && chmod 440 /etc/sudoers.d/runner \
    && mkdir -p /opt/hostedtoolcache /opt/ferrodock-cache /opt/ferrodock-artifacts \
    && chown runner:runner /opt/hostedtoolcache /opt/ferrodock-cache /opt/ferrodock-artifacts
# A Node LTS, as the runners have one.
RUN arch=$(uname -m | sed 's/x86_64/x64/;s/aarch64/arm64/') \
    && file=$(curl -fsSL https://nodejs.org/dist/latest-v22.x/ | grep -o "node-v22[^\"]*-linux-$arch.tar.xz" | head -1) \
    && curl -fsSL "https://nodejs.org/dist/latest-v22.x/$file" | tar -xJ -C /usr/local --strip-components=1
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
        assert_eq!(tag("24.04"), "ferrodock-ci-runner:24.04-1");
    }
}
