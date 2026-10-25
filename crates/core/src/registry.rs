//! Working with registries and image files: logins from the Docker CLI's
//! config, Docker Hub search, tagging, pushing, and saving and loading
//! images as `.tar` files.

use std::path::{Path, PathBuf};

use bollard::auth::DockerCredentials;
use bollard::query_parameters::{
    ImportImageOptions, PushImageOptions, SearchImagesOptions, TagImageOptions,
};
use futures_util::StreamExt;
use serde_json::Value;

use super::run::PullProgress;
use super::{DockerClient, describe};

/// The key Docker Hub's login is stored under.
pub(crate) const DOCKER_HUB: &str = "https://index.docker.io/v1/";

/// A Docker Hub search result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchResult {
    pub name: String,
    pub description: String,
    pub stars: i64,
    pub official: bool,
}

/// Something to do with images beyond pulling and running.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageOp {
    Search(String),
    /// Give `image` the name `target` (`repo[:tag]`).
    Tag {
        image: String,
        target: String,
    },
    Push(String),
    /// Save `image` to a `.tar` in the Downloads folder.
    Save(String),
    /// Load the images in a `.tar` file.
    Load(PathBuf),
    /// Build an image from a Dockerfile; its output streams like a
    /// Compose job's.
    Build(super::BuildSpec),
}

/// The registry an image reference lives on, as the Docker config keys it:
/// `ghcr.io/me/app` → `ghcr.io`, `nginx` → Docker Hub.
pub fn registry_of(reference: &str) -> String {
    match reference.split_once('/') {
        Some((first, _)) if first.contains('.') || first.contains(':') || first == "localhost" => {
            first.to_owned()
        }
        _ => DOCKER_HUB.to_owned(),
    }
}

/// `repo:tag` → (`repo`, `tag`); no tag means `latest`.
fn split_reference(reference: &str) -> (&str, &str) {
    match reference.rsplit_once(':') {
        Some((repo, tag)) if !tag.contains('/') => (repo, tag),
        _ => (reference, "latest"),
    }
}

/// A file name for a saved image: `ghcr.io/me/app:1.0` → `app-1.0.tar`.
pub fn tar_name(reference: &str) -> String {
    let (repo, tag) = split_reference(reference);
    let name = repo.rsplit('/').next().unwrap_or(repo);
    let clean = |s: &str| s.replace(|c: char| !c.is_ascii_alphanumeric() && c != '.', "-");
    if reference.starts_with("sha256:") {
        format!("image-{}.tar", &reference[7..19.min(reference.len())])
    } else {
        format!("{}-{}.tar", clean(name), clean(tag))
    }
}

/// Drops emoji (and the joiners around them), which the app's fonts can't
/// draw; registry descriptions are full of them.
fn without_emoji(text: &str) -> String {
    let kept: String = text
        .chars()
        .filter(|c| {
            !matches!(*c as u32,
                0x1F000..=0x1FAFF | 0x2600..=0x27BF | 0xFE00..=0xFE0F | 0x200D | 0x2B00..=0x2BFF)
        })
        .collect();
    kept.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The Docker CLI's config file.
pub(crate) fn config_path() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("DOCKER_CONFIG") {
        return Some(PathBuf::from(dir).join("config.json"));
    }
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    Some(PathBuf::from(home).join(".docker").join("config.json"))
}

/// The login for `registry` from a parsed Docker config, using a
/// credential helper when the config names one.
fn credentials_in(config: &Value, registry: &str) -> Option<DockerCredentials> {
    let helper = config["credHelpers"][registry]
        .as_str()
        .or_else(|| config["credsStore"].as_str());
    if let Some(helper) = helper
        && let Some(creds) = from_helper(helper, registry)
    {
        return Some(creds);
    }
    let auth = config["auths"][registry]["auth"].as_str()?;
    let decoded = base64_decode(auth)?;
    let (username, password) = decoded.split_once(':')?;
    Some(DockerCredentials {
        username: Some(username.to_owned()),
        password: Some(password.to_owned()),
        serveraddress: Some(registry.to_owned()),
        ..Default::default()
    })
}

/// Asks `docker-credential-<helper>` for the login of `registry`.
fn from_helper(helper: &str, registry: &str) -> Option<DockerCredentials> {
    use std::io::Write;
    use std::process::Stdio;
    let mut child = helper_command(helper)
        .arg("get")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    child.stdin.take()?.write_all(registry.as_bytes()).ok()?;
    let output = child.wait_with_output().ok()?;
    if !output.status.success() {
        return None;
    }
    let value: Value = serde_json::from_slice(&output.stdout).ok()?;
    let username = value["Username"].as_str()?.to_owned();
    let secret = value["Secret"].as_str()?.to_owned();
    // An identity token, not a password.
    Some(if username == "<token>" {
        DockerCredentials {
            identitytoken: Some(secret),
            serveraddress: Some(registry.to_owned()),
            ..Default::default()
        }
    } else {
        DockerCredentials {
            username: Some(username),
            password: Some(secret),
            serveraddress: Some(registry.to_owned()),
            ..Default::default()
        }
    })
}

/// The command for `docker-credential-<helper>`, with no window flashing
/// up on Windows.
pub(crate) fn helper_command(helper: &str) -> std::process::Command {
    #[allow(unused_mut)]
    let mut command = std::process::Command::new(format!("docker-credential-{helper}"));
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

/// The (registry, username) pairs a credential helper keeps.
pub(crate) fn from_helper_list(helper: &str) -> Vec<(String, String)> {
    let Ok(output) = helper_command(helper)
        .arg("list")
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
    else {
        return Vec::new();
    };
    serde_json::from_slice::<std::collections::BTreeMap<String, String>>(&output.stdout)
        .map(|map| map.into_iter().collect())
        .unwrap_or_default()
}

/// The login to use for `reference`, if the Docker CLI has one.
pub fn credentials_for(reference: &str) -> Option<DockerCredentials> {
    let text = std::fs::read_to_string(config_path()?).ok()?;
    let config: Value = serde_json::from_str(&text).ok()?;
    credentials_in(&config, &registry_of(reference))
}

/// Standard base64, as the Docker config stores logins.
pub(crate) fn base64_decode(input: &str) -> Option<String> {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut bits = 0u32;
    let mut count = 0;
    let mut out = Vec::new();
    for byte in input.trim_end_matches('=').bytes() {
        let value = ALPHABET.iter().position(|b| *b == byte)? as u32;
        bits = (bits << 6) | value;
        count += 6;
        if count >= 8 {
            count -= 8;
            out.push((bits >> count) as u8);
            bits &= (1 << count) - 1;
        }
    }
    String::from_utf8(out).ok()
}

impl DockerClient {
    pub async fn search_images(&self, term: &str) -> Result<Vec<SearchResult>, String> {
        let results = self
            .docker
            .search_images(SearchImagesOptions {
                term: term.to_owned(),
                limit: Some(25),
                ..Default::default()
            })
            .await
            .map_err(describe)?;
        let mut results: Vec<SearchResult> = results
            .into_iter()
            .map(|r| SearchResult {
                name: r.name.unwrap_or_default(),
                description: without_emoji(&r.description.unwrap_or_default()),
                stars: r.star_count.unwrap_or_default(),
                official: r.is_official.unwrap_or_default(),
            })
            .collect();
        results.sort_by_key(|r| (!r.official, -r.stars));
        Ok(results)
    }

    pub async fn tag_image(&self, image: &str, target: &str) -> Result<(), String> {
        let (repo, tag) = split_reference(target.trim());
        self.docker
            .tag_image(
                image,
                Some(TagImageOptions {
                    repo: Some(repo.to_owned()),
                    tag: Some(tag.to_owned()),
                }),
            )
            .await
            .map_err(describe)
    }

    /// Pushes `reference` with the Docker CLI's login for its registry,
    /// calling `progress` as it goes.
    pub async fn push_image(
        &self,
        reference: &str,
        mut progress: impl FnMut(PullProgress),
    ) -> Result<(), String> {
        let (repo, tag) = split_reference(reference);
        let mut stream = self.docker.push_image(
            repo,
            Some(PushImageOptions {
                tag: Some(tag.to_owned()),
                ..Default::default()
            }),
            credentials_for(reference),
        );
        while let Some(item) = stream.next().await {
            let info = item.map_err(describe)?;
            if let Some(error) = info.error_detail.and_then(|e| e.message) {
                return Err(error);
            }
            let detail = info.progress_detail.unwrap_or_default();
            progress(PullProgress {
                done: detail.current.unwrap_or_default().max(0) as u64,
                total: detail.total.unwrap_or_default().max(0) as u64,
                status: info.status.unwrap_or_default(),
                pushing: true,
            });
        }
        Ok(())
    }

    /// Saves `image` as a `.tar` in `dir`, under a name nothing else has.
    pub async fn save_image(&self, image: &str, dir: &Path) -> Result<PathBuf, String> {
        let path = super::files::unique_path(dir, &tar_name(image));
        let mut file = std::fs::File::create(&path).map_err(|e| e.to_string())?;
        let mut stream = self.docker.export_image(image);
        while let Some(chunk) = stream.next().await {
            let chunk = match chunk {
                Ok(chunk) => chunk,
                Err(e) => {
                    let _ = std::fs::remove_file(&path);
                    return Err(describe(e));
                }
            };
            std::io::Write::write_all(&mut file, &chunk).map_err(|e| e.to_string())?;
        }
        Ok(path)
    }

    /// Loads the images in the `.tar` at `path`. Returns what Docker said
    /// it loaded, e.g. "Loaded image: nginx:alpine".
    pub async fn load_image(&self, path: &Path) -> Result<Vec<String>, String> {
        let file = tokio::fs::File::open(path)
            .await
            .map_err(|e| format!("Couldn't open {}: {e}", path.display()))?;
        let chunks = futures_util::stream::unfold(file, |mut file| async move {
            use tokio::io::AsyncReadExt;
            let mut buffer = vec![0; 1 << 20];
            match file.read(&mut buffer).await {
                Ok(0) => None,
                Ok(n) => {
                    buffer.truncate(n);
                    Some((Ok::<_, std::io::Error>(bytes::Bytes::from(buffer)), file))
                }
                Err(e) => Some((Err(e), file)),
            }
        });
        let mut stream =
            self.docker
                .import_image_stream(ImportImageOptions::default(), chunks, None);
        let mut loaded = Vec::new();
        while let Some(item) = stream.next().await {
            let info = item.map_err(describe)?;
            if let Some(error) = info.error_detail.and_then(|e| e.message) {
                return Err(error);
            }
            if let Some(line) = info
                .stream
                .as_deref()
                .map(str::trim)
                .filter(|l| !l.is_empty())
            {
                loaded.push(line.to_owned());
            }
        }
        Ok(loaded)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_registry() {
        assert_eq!(registry_of("nginx:alpine"), DOCKER_HUB);
        assert_eq!(registry_of("me/app"), DOCKER_HUB);
        assert_eq!(registry_of("ghcr.io/me/app:1"), "ghcr.io");
        assert_eq!(registry_of("localhost:5000/app"), "localhost:5000");
        assert_eq!(registry_of("localhost/app"), "localhost");
    }

    #[test]
    fn drops_emoji() {
        assert_eq!(without_emoji("🐳 Docker ✨ images"), "Docker images");
    }

    #[test]
    fn names_saved_images() {
        assert_eq!(tar_name("nginx:alpine"), "nginx-alpine.tar");
        assert_eq!(tar_name("ghcr.io/me/app"), "app-latest.tar");
        assert_eq!(
            tar_name("sha256:abcdef0123456789"),
            "image-abcdef012345.tar"
        );
    }

    #[test]
    fn reads_logins_from_the_config() {
        // "me:secret" in base64.
        let config: Value =
            serde_json::from_str(r#"{"auths": {"ghcr.io": {"auth": "bWU6c2VjcmV0"}}}"#).unwrap();
        let creds = credentials_in(&config, "ghcr.io").unwrap();
        assert_eq!(creds.username.as_deref(), Some("me"));
        assert_eq!(creds.password.as_deref(), Some("secret"));
        assert!(credentials_in(&config, DOCKER_HUB).is_none());
        assert_eq!(base64_decode("aGk=").as_deref(), Some("hi"));
    }

    /// Talks to a real daemon: `cargo test -- --ignored`. Tags an image,
    /// saves it, removes the tag, loads it back, and pushes to a registry
    /// that isn't there (which must fail cleanly).
    #[tokio::test]
    #[ignore]
    async fn live_daemon_tags_saves_and_loads() {
        use crate::{Action, Resource};
        let client = DockerClient::connect().unwrap();
        let Some(image) = client.list_images().await.unwrap().into_iter().next() else {
            return;
        };
        let name = "ferrodock-test/saved:1";
        client.tag_image(&image.id, name).await.unwrap();
        let dir = std::env::temp_dir().join(format!("ferrodock-save-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let saved = client.save_image(name, &dir).await.unwrap();
        assert_eq!(saved, dir.join("saved-1.tar"));
        assert!(std::fs::metadata(&saved).unwrap().len() > 0);
        // Untag, then load it back.
        client
            .perform(Action::Remove(Resource::Images), name)
            .await
            .ok();
        let loaded = client.load_image(&saved).await.unwrap();
        let tags: Vec<String> = client
            .list_images()
            .await
            .unwrap()
            .into_iter()
            .flat_map(|i| i.tags)
            .collect();
        let pushed = client
            .push_image("localhost:1/ferrodock-test:1", |_| {})
            .await;
        std::fs::remove_dir_all(&dir).unwrap();
        client
            .docker
            .remove_image(
                name,
                None::<bollard::query_parameters::RemoveImageOptions>,
                None,
            )
            .await
            .ok();
        assert!(loaded.iter().any(|l| l.contains(name)), "{loaded:?}");
        assert!(tags.iter().any(|t| t == name));
        assert!(pushed.is_err(), "nothing listens on localhost:1");
    }
}
