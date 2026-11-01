//! Actions from GitHub: what a `uses:` names, and downloading it once into
//! Ferrodock's folder (`ci-actions/<owner>/<repo>/<ref>`). Moving refs,
//! like `v4` or `main`, are fetched again after a day; a commit SHA never.

use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, SystemTime};

/// How long a downloaded tag or branch is used before it's fetched again.
const FRESH_FOR: Duration = Duration::from_secs(24 * 3600);

/// Where the action of a `uses:` comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// `./path`: a folder of the project.
    Local(String),
    /// `owner/repo[/path]@ref`.
    GitHub {
        owner: String,
        repo: String,
        path: String,
        git_ref: String,
    },
    /// `docker://image`.
    Image(String),
}

impl Source {
    pub fn parse(uses: &str) -> Result<Self, String> {
        let uses = uses.trim();
        if let Some(image) = uses.strip_prefix("docker://") {
            return Ok(Source::Image(image.to_owned()));
        }
        if uses.starts_with("./") {
            return Ok(Source::Local(uses.trim_start_matches("./").to_owned()));
        }
        let (name, git_ref) = uses
            .split_once('@')
            .ok_or_else(|| format!("{uses} names no version (@v1, @main, a commit…)"))?;
        let mut parts = name.splitn(3, '/');
        let (owner, repo) = match (parts.next(), parts.next()) {
            (Some(o), Some(r)) if !o.is_empty() && !r.is_empty() => (o, r),
            _ => return Err(format!("{uses} isn't owner/repo@version")),
        };
        let path = parts.next().unwrap_or("").trim_matches('/').to_owned();
        let safe = |s: &str| !s.is_empty() && !s.contains("..") && !s.contains('\\');
        if !safe(owner) || !safe(repo) || !safe(git_ref) || path.contains("..") {
            return Err(format!("{uses} isn't owner/repo@version"));
        }
        Ok(Source::GitHub {
            owner: owner.to_owned(),
            repo: repo.to_owned(),
            path,
            git_ref: git_ref.to_owned(),
        })
    }
}

/// The folder an action of GitHub is in, downloading it if needed. Blocks.
/// With `token` (the run's `GITHUB_TOKEN` secret), private actions work
/// too.
pub fn fetch(
    owner: &str,
    repo: &str,
    git_ref: &str,
    token: Option<&str>,
) -> Result<PathBuf, String> {
    let root = super::data_dir()
        .ok_or("there's no home folder")?
        .join("ci-actions");
    fetch_into(&root, owner, repo, git_ref, token)
}

fn fetch_into(
    root: &Path,
    owner: &str,
    repo: &str,
    git_ref: &str,
    token: Option<&str>,
) -> Result<PathBuf, String> {
    let dir = root
        .join(owner.to_lowercase())
        .join(repo.to_lowercase())
        .join(git_ref.replace('/', "%2F"));
    let marker = dir.join(".ferrodock-fetched");
    let pinned = git_ref.len() == 40 && git_ref.chars().all(|c| c.is_ascii_hexdigit());
    let fresh = std::fs::metadata(&marker)
        .and_then(|m| m.modified())
        .is_ok_and(|at| {
            pinned
                || SystemTime::now()
                    .duration_since(at)
                    .is_ok_and(|age| age < FRESH_FOR)
        });
    if fresh {
        return Ok(dir);
    }
    let bytes = match download(owner, repo, git_ref, token) {
        Ok(bytes) => bytes,
        // Offline: an older copy still works.
        Err(_) if marker.exists() => return Ok(dir),
        Err(e) => return Err(e),
    };
    let parent = dir.parent().ok_or("bad action folder")?;
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let staging = parent.join(format!(".download-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&staging);
    unpack(&bytes, &staging)?;
    std::fs::write(staging.join(".ferrodock-fetched"), git_ref).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::rename(&staging, &dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

/// Trusts what the system trusts (and `SSL_CERT_FILE`), so company
/// proxies with their own certificate authority work.
fn agent() -> ureq::Agent {
    use ureq::tls::{Certificate, RootCerts, TlsConfig};
    let found = rustls_native_certs::load_native_certs();
    let roots = if found.certs.is_empty() {
        RootCerts::WebPki
    } else {
        let certs = found
            .certs
            .iter()
            .map(|c| Certificate::from_der(c.as_ref()).to_owned())
            .collect();
        RootCerts::Specific(std::sync::Arc::new(certs))
    };
    ureq::Agent::config_builder()
        .tls_config(TlsConfig::builder().root_certs(roots).build())
        .build()
        .new_agent()
}

fn download(
    owner: &str,
    repo: &str,
    git_ref: &str,
    token: Option<&str>,
) -> Result<Vec<u8>, String> {
    let what = format!("{owner}/{repo}@{git_ref}");
    let agent = agent();
    let response = match token.filter(|t| !t.is_empty()) {
        Some(token) => agent
            .get(&format!(
                "https://api.github.com/repos/{owner}/{repo}/tarball/{git_ref}"
            ))
            .header("Authorization", &format!("Bearer {token}"))
            .header("User-Agent", "Ferrodock")
            .call(),
        None => agent
            .get(&format!(
                "https://codeload.github.com/{owner}/{repo}/tar.gz/{git_ref}"
            ))
            .header("User-Agent", "Ferrodock")
            .call(),
    };
    let mut response = response.map_err(|e| match e {
        ureq::Error::StatusCode(404) => format!(
            "{what} wasn't found on GitHub (for a private action, add a GITHUB_TOKEN secret)"
        ),
        e => format!("{what} couldn't be downloaded: {e}"),
    })?;
    let mut bytes = Vec::new();
    response
        .body_mut()
        .with_config()
        .limit(500 * 1024 * 1024)
        .reader()
        .read_to_end(&mut bytes)
        .map_err(|e| format!("{what} couldn't be downloaded: {e}"))?;
    Ok(bytes)
}

/// Unpacks GitHub's tarball into `dir`, without its top folder. Only
/// files and folders, and nothing outside `dir`.
fn unpack(tar_gz: &[u8], dir: &Path) -> Result<(), String> {
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(tar_gz));
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    for entry in archive.entries().map_err(|e| e.to_string())? {
        let mut entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path().map_err(|e| e.to_string())?.into_owned();
        let inner: PathBuf = path.components().skip(1).collect();
        if inner.as_os_str().is_empty()
            || inner
                .components()
                .any(|c| !matches!(c, Component::Normal(_)))
        {
            continue;
        }
        let target = dir.join(&inner);
        match entry.header().entry_type() {
            tar::EntryType::Directory => {
                std::fs::create_dir_all(&target).map_err(|e| e.to_string())?;
            }
            tar::EntryType::Regular | tar::EntryType::Continuous => {
                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                }
                entry.unpack(&target).map_err(|e| e.to_string())?;
            }
            // Links could point anywhere; actions don't need them.
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_uses() {
        assert_eq!(
            Source::parse("actions/github-script@v7"),
            Ok(Source::GitHub {
                owner: "actions".into(),
                repo: "github-script".into(),
                path: String::new(),
                git_ref: "v7".into(),
            })
        );
        assert_eq!(
            Source::parse("github/codeql-action/init@v3").unwrap(),
            Source::GitHub {
                owner: "github".into(),
                repo: "codeql-action".into(),
                path: "init".into(),
                git_ref: "v3".into(),
            }
        );
        assert_eq!(
            Source::parse("./.github/actions/x"),
            Ok(Source::Local(".github/actions/x".into()))
        );
        assert_eq!(
            Source::parse("docker://alpine:3"),
            Ok(Source::Image("alpine:3".into()))
        );
        assert!(Source::parse("actions/checkout").is_err());
        assert!(Source::parse("a/../b@v1").is_err());
    }

    #[test]
    fn unpacks_without_the_top_folder_or_escapes() {
        let mut tar = tar::Builder::new(Vec::new());
        for (path, body) in [
            ("repo-v1/action.yml", "runs: {}"),
            ("repo-v1/dist/index.js", "1"),
        ] {
            let mut h = tar::Header::new_gnu();
            h.set_size(body.len() as u64);
            h.set_mode(0o644);
            h.set_cksum();
            tar.append_data(&mut h, path, body.as_bytes()).unwrap();
        }
        // An entry trying to climb out.
        let mut h = tar::Header::new_gnu();
        h.set_size(1);
        h.set_mode(0o644);
        let evil = b"repo-v1/../../evil";
        h.as_gnu_mut().unwrap().name[..evil.len()].copy_from_slice(evil);
        h.set_cksum();
        tar.append(&h, &b"x"[..]).unwrap();
        let raw = tar.into_inner().unwrap();
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        std::io::Write::write_all(&mut gz, &raw).unwrap();
        let bytes = gz.finish().unwrap();

        let dir = std::env::temp_dir().join(format!("ferrodock-fetch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        unpack(&bytes, &dir.join("out")).unwrap();
        assert!(dir.join("out/action.yml").is_file());
        assert!(dir.join("out/dist/index.js").is_file());
        assert!(!dir.join("evil").exists() && !dir.parent().unwrap().join("evil").exists());
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[cfg(test)]
mod live {
    /// Downloads a real action: `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn live_downloads_an_action() {
        let root = std::env::temp_dir().join(format!("ferrodock-actions-{}", std::process::id()));
        let dir = super::fetch_into(
            &root,
            "actions",
            "hello-world-javascript-action",
            "main",
            None,
        )
        .unwrap();
        let action = super::super::metadata::read(&dir).unwrap();
        assert!(
            matches!(action.runs, super::super::metadata::Runs::Node { .. }),
            "{action:?}"
        );
        // Fresh now: the second call doesn't download.
        assert_eq!(
            super::fetch_into(
                &root,
                "actions",
                "hello-world-javascript-action",
                "main",
                None
            )
            .unwrap(),
            dir
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
