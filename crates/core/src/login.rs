//! Signing in to registries, like `docker login`: the login is checked
//! with the registry, then kept where the Docker CLI keeps it (a
//! credential helper when there is one, otherwise its config file), so
//! the CLI, Compose and Ferrodock all use it.

use std::path::Path;

use serde_json::{Map, Value};

use super::registry::{DOCKER_HUB, config_path, from_helper_list, helper_command};

/// A registry Ferrodock (or the Docker CLI) is signed in to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Login {
    /// As the Docker config keys it, e.g. `ghcr.io`.
    pub registry: String,
    /// Empty when the config doesn't say.
    pub username: String,
}

impl Login {
    /// What to call the registry: "Docker Hub" for its long key.
    pub fn label(&self) -> &str {
        registry_label(&self.registry)
    }
}

/// "Docker Hub" for Docker Hub's key, the registry otherwise.
pub fn registry_label(registry: &str) -> &str {
    if registry == DOCKER_HUB {
        "Docker Hub"
    } else {
        registry
    }
}

/// The key the Docker config uses for what was typed: empty, `docker.io`
/// and the like mean Docker Hub; a URL loses its scheme and path.
pub fn registry_key(typed: &str) -> String {
    let typed = typed.trim();
    let host = typed
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .split('/')
        .next()
        .unwrap_or_default();
    match host {
        "" | "docker.io" | "index.docker.io" | "registry-1.docker.io" | "hub.docker.com" => {
            DOCKER_HUB.to_owned()
        }
        host => host.to_owned(),
    }
}

/// The registries in the Docker config, with their usernames.
pub fn logins() -> Vec<Login> {
    let config = read_config();
    let mut logins: Vec<Login> = Vec::new();
    let mut add =
        |registry: &str, username: String| match logins.iter_mut().find(|l| l.registry == registry)
        {
            Some(login) if login.username.is_empty() => login.username = username,
            Some(_) => {}
            None => logins.push(Login {
                registry: registry.to_owned(),
                username,
            }),
        };
    if let Some(store) = config["credsStore"].as_str() {
        for (registry, username) in from_helper_list(store) {
            add(&registry, username);
        }
    }
    if let Some(helpers) = config["credHelpers"].as_object() {
        for (registry, helper) in helpers {
            let username = helper
                .as_str()
                .and_then(|h| from_helper_list(h).into_iter().find(|(r, _)| r == registry))
                .map(|(_, u)| u)
                .unwrap_or_default();
            add(registry, username);
        }
    }
    if let Some(auths) = config["auths"].as_object() {
        for (registry, entry) in auths {
            let username = entry["auth"]
                .as_str()
                .and_then(super::registry::base64_decode)
                .and_then(|d| d.split_once(':').map(|(u, _)| u.to_owned()));
            // An entry without "auth" is only a marker the CLI leaves.
            if let Some(username) = username {
                add(registry, username);
            }
        }
    }
    logins.sort_by(|a, b| {
        (a.registry != DOCKER_HUB, &a.registry).cmp(&(b.registry != DOCKER_HUB, &b.registry))
    });
    logins
}

/// Checks the login with `registry` (a config key, see [`registry_key`]),
/// then saves it. Returns where it went, for the user.
pub fn sign_in(registry: &str, username: &str, password: &str) -> Result<String, String> {
    if username.trim().is_empty() || password.is_empty() {
        return Err("Enter a username and a password or token".into());
    }
    verify(registry, username.trim(), password)?;
    save(registry, username.trim(), password)
}

/// Forgets the login for `registry`, wherever it's kept.
pub fn sign_out(registry: &str) -> Result<(), String> {
    let mut config = read_config();
    if let Some(helper) = helper_for(&config, registry) {
        // Not signed in there is fine too.
        let _ = run_helper(&helper, "erase", registry);
    }
    if let Some(auths) = config["auths"].as_object_mut() {
        auths.remove(registry);
    }
    write_config(&config)
}

/// Where the registry's API lives: Docker Hub's isn't at its config key.
fn api_base(registry: &str) -> String {
    if registry == DOCKER_HUB {
        return "https://registry-1.docker.io".into();
    }
    let local = registry.starts_with("localhost") || registry.starts_with("127.");
    format!("{}://{registry}", if local { "http" } else { "https" })
}

/// Logs in to the registry's API the way the Docker CLI does: `/v2/`
/// answers with how to authenticate, Basic or a Bearer token service.
fn verify(registry: &str, username: &str, password: &str) -> Result<(), String> {
    let base = api_base(registry);
    let basic = format!("Basic {}", base64_encode(&format!("{username}:{password}")));
    let get = |url: &str, auth: Option<&str>| {
        let mut request = ureq::get(url).config().http_status_as_error(false).build();
        if let Some(auth) = auth {
            request = request.header("Authorization", auth);
        }
        request
            .call()
            .map_err(|e| format!("Couldn't reach {registry}: {e}"))
    };
    let probe = get(&format!("{base}/v2/"), None)?;
    if probe.status() != 401 {
        // Open to everyone: nothing to check.
        return Ok(());
    }
    let challenge = probe
        .headers()
        .get("www-authenticate")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    let answer = match bearer_url(&challenge) {
        Some(token_url) => get(&token_url, Some(&basic))?,
        None => get(&format!("{base}/v2/"), Some(&basic))?,
    };
    match answer.status().as_u16() {
        200..=299 => Ok(()),
        401 | 403 => Err("The username or password is wrong".into()),
        status => Err(format!("{registry} answered {status}")),
    }
}

/// The token URL from a `Bearer realm="…",service="…",scope="…"`
/// challenge, None for Basic.
fn bearer_url(challenge: &str) -> Option<String> {
    let params = challenge.strip_prefix("Bearer ")?;
    let mut realm = None;
    let mut query = Vec::new();
    // Values are quoted and may hold commas (scopes), so split carefully.
    let mut rest = params.trim();
    while let Some((key, after)) = rest.split_once('=') {
        let key = key.trim().trim_start_matches(',').trim();
        let after = after.trim_start();
        let (value, next) = match after.strip_prefix('"') {
            Some(quoted) => {
                let end = quoted.find('"')?;
                (&quoted[..end], &quoted[end + 1..])
            }
            None => after.split_once(',').unwrap_or((after, "")),
        };
        match key {
            "realm" => realm = Some(value.to_owned()),
            "service" | "scope" => query.push(format!("{key}={}", value.replace(' ', "%20"))),
            _ => {}
        }
        rest = next;
    }
    let realm = realm?;
    Some(if query.is_empty() {
        realm
    } else {
        format!(
            "{realm}{}{}",
            if realm.contains('?') { '&' } else { '?' },
            query.join("&")
        )
    })
}

/// Saves the login: with the registry's credential helper, else the
/// default one for this system if it's installed (and noted in the
/// config, as the CLI does), else in the config file itself.
fn save(registry: &str, username: &str, password: &str) -> Result<String, String> {
    let mut config = read_config();
    let helper = helper_for(&config, registry).or_else(|| {
        let found = default_helper()?;
        config["credsStore"] = Value::String(found.clone());
        Some(found)
    });
    if let Some(helper) = &helper {
        let secret = serde_json::json!({
            "ServerURL": registry,
            "Username": username,
            "Secret": password,
        });
        run_helper(helper, "store", &secret.to_string())?;
        // The CLI leaves an empty entry so `docker login` lists it.
        object(&mut config, "auths").insert(registry.to_owned(), Value::Object(Map::new()));
        write_config(&config)?;
        return Ok(format!("saved with docker-credential-{helper}"));
    }
    let auth = base64_encode(&format!("{username}:{password}"));
    object(&mut config, "auths").insert(registry.to_owned(), serde_json::json!({ "auth": auth }));
    write_config(&config)?;
    Ok("saved, unencrypted, in the Docker config file".into())
}

/// The helper the config names for `registry`, if any.
fn helper_for(config: &Value, registry: &str) -> Option<String> {
    config["credHelpers"][registry]
        .as_str()
        .or_else(|| config["credsStore"].as_str())
        .filter(|h| !h.is_empty())
        .map(str::to_owned)
}

/// The system's keychain helper, when it's installed.
fn default_helper() -> Option<String> {
    let candidates: &[&str] = if cfg!(target_os = "macos") {
        &["osxkeychain", "desktop"]
    } else if cfg!(windows) {
        &["wincred", "desktop"]
    } else {
        &["secretservice", "pass"]
    };
    candidates
        .iter()
        .find(|h| helper_command(h).arg("version").output().is_ok())
        .map(|h| (*h).to_owned())
}

fn run_helper(helper: &str, action: &str, input: &str) -> Result<(), String> {
    use std::io::Write;
    use std::process::Stdio;
    let mut child = helper_command(helper)
        .arg(action)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("docker-credential-{helper}: {e}"))?;
    child
        .stdin
        .take()
        .ok_or("no stdin")?
        .write_all(input.as_bytes())
        .map_err(|e| e.to_string())?;
    let output = child.wait_with_output().map_err(|e| e.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        let said = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        Err(format!("docker-credential-{helper} {action}: {said}"))
    }
}

/// The object at `key` in the config, made if missing.
fn object<'a>(config: &'a mut Value, key: &str) -> &'a mut Map<String, Value> {
    if !config[key].is_object() {
        config[key] = Value::Object(Map::new());
    }
    config[key].as_object_mut().expect("just made it an object")
}

fn read_config() -> Value {
    config_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|t| serde_json::from_str(&t).ok())
        .filter(Value::is_object)
        .unwrap_or_else(|| Value::Object(Map::new()))
}

/// Writes the config (only readable by this user), through a temporary
/// file so a half-written one never replaces it.
fn write_config(config: &Value) -> Result<(), String> {
    let path = config_path().ok_or("no home folder")?;
    write_config_to(&path, config)
}

fn write_config_to(path: &Path, config: &Value) -> Result<(), String> {
    let dir = path.parent().ok_or("no folder for the Docker config")?;
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let text = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    let partial = path.with_extension("json.partial");
    std::fs::write(&partial, text + "\n").map_err(|e| format!("{}: {e}", partial.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&partial, std::fs::Permissions::from_mode(0o600));
    }
    std::fs::rename(&partial, path).map_err(|e| format!("{}: {e}", path.display()))
}

/// Standard base64, as the Docker config stores logins.
fn base64_encode(text: &str) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in text.as_bytes().chunks(3) {
        let n = chunk.iter().fold(0u32, |n, b| (n << 8) | u32::from(*b)) << (8 * (3 - chunk.len()));
        for i in 0..4 {
            out.push(if i <= chunk.len() {
                ALPHABET[(n >> (18 - 6 * i)) as usize & 63] as char
            } else {
                '='
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_registries_like_the_cli() {
        for hub in [
            "",
            "docker.io",
            "https://index.docker.io/v1/",
            "registry-1.docker.io",
        ] {
            assert_eq!(registry_key(hub), DOCKER_HUB, "{hub}");
        }
        assert_eq!(registry_key("https://ghcr.io/me"), "ghcr.io");
        assert_eq!(registry_key(" localhost:5000 "), "localhost:5000");
        assert_eq!(registry_label(DOCKER_HUB), "Docker Hub");
        assert_eq!(api_base("localhost:5000"), "http://localhost:5000");
        assert_eq!(api_base("ghcr.io"), "https://ghcr.io");
    }

    #[test]
    fn encodes_base64() {
        for (text, encoded) in [
            ("me:secret", "bWU6c2VjcmV0"),
            ("a", "YQ=="),
            ("ab", "YWI="),
            ("", ""),
        ] {
            assert_eq!(base64_encode(text), encoded);
            assert_eq!(
                super::super::registry::base64_decode(encoded).as_deref(),
                Some(text)
            );
        }
    }

    #[test]
    fn reads_token_challenges() {
        assert_eq!(
            bearer_url(
                r#"Bearer realm="https://auth.docker.io/token",service="registry.docker.io""#
            )
            .as_deref(),
            Some("https://auth.docker.io/token?service=registry.docker.io")
        );
        assert_eq!(
            bearer_url(r#"Bearer realm="https://ghcr.io/token",service="ghcr.io",scope="repository:a/b:pull,push""#)
                .as_deref(),
            Some("https://ghcr.io/token?service=ghcr.io&scope=repository:a/b:pull,push")
        );
        assert_eq!(bearer_url(r#"Basic realm="Registry""#), None);
    }

    #[test]
    fn writes_the_config_keeping_what_is_there() {
        let dir = std::env::temp_dir().join(format!("ferrodock-login-{}", std::process::id()));
        let path = dir.join("config.json");
        let mut config: Value = serde_json::from_str(
            r#"{"auths": {"ghcr.io": {"auth": "bWU6c2VjcmV0"}}, "currentContext": "x"}"#,
        )
        .unwrap();
        object(&mut config, "auths").insert("quay.io".into(), serde_json::json!({"auth": "YQ=="}));
        write_config_to(&path, &config).unwrap();
        let back: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(back["currentContext"], "x");
        assert_eq!(back["auths"]["ghcr.io"]["auth"], "bWU6c2VjcmV0");
        assert_eq!(back["auths"]["quay.io"]["auth"], "YQ==");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// Talks to Docker Hub and GitHub's registry: `cargo test -- --ignored`.
    /// A wrong password is refused as such, not as a network error.
    #[test]
    #[ignore]
    fn live_registries_refuse_a_wrong_password() {
        for registry in [DOCKER_HUB, "ghcr.io"] {
            let error = verify(registry, "ferrodock-test-nobody", "not-the-password").unwrap_err();
            assert_eq!(error, "The username or password is wrong", "{registry}");
        }
    }

    /// Signs in to and out of a local registry with Basic auth:
    /// `TEST_REGISTRY=localhost:5000 TEST_LOGIN=user:password
    /// DOCKER_CONFIG=<scratch folder> cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn live_signs_in_and_out() {
        let (Ok(registry), Ok(login), Ok(_)) = (
            std::env::var("TEST_REGISTRY"),
            std::env::var("TEST_LOGIN"),
            std::env::var("DOCKER_CONFIG"),
        ) else {
            return;
        };
        let (user, password) = login.split_once(':').unwrap();
        assert_eq!(
            sign_in(&registry, user, "wrong").unwrap_err(),
            "The username or password is wrong"
        );
        sign_in(&registry, user, password).unwrap();
        assert!(
            logins()
                .iter()
                .any(|l| l.registry == registry && l.username == user)
        );
        let creds = super::super::registry::credentials_for(&format!("{registry}/app")).unwrap();
        assert_eq!(creds.password.as_deref(), Some(password));
        sign_out(&registry).unwrap();
        assert!(!logins().iter().any(|l| l.registry == registry));
    }
}
