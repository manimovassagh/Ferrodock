//! The `github` and `runner` contexts, and the event a run pretends to be,
//! from the project's `.git` (read directly; git isn't needed).

use std::path::Path;

use serde_json::{Value, json};

/// What `.git` says about the project.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GitInfo {
    /// e.g. `refs/heads/main`.
    pub git_ref: String,
    pub sha: String,
    /// `owner/name`, from the `origin` remote, or the folder's name.
    pub repository: String,
}

pub fn git_info(project: &Path) -> GitInfo {
    let git = project.join(".git");
    let read = |p: &str| std::fs::read_to_string(git.join(p)).unwrap_or_default();
    let head = read("HEAD");
    let head = head.trim();
    let (git_ref, sha) = match head.strip_prefix("ref: ") {
        Some(r) => {
            let sha = read(r).trim().to_owned();
            let sha = if sha.is_empty() {
                read("packed-refs")
                    .lines()
                    .find_map(|l| l.strip_suffix(r).map(|s| s.trim().to_owned()))
                    .unwrap_or_default()
            } else {
                sha
            };
            (r.to_owned(), sha)
        }
        // Detached: no branch.
        None => (String::new(), head.to_owned()),
    };
    let folder = project.file_name().map_or_else(
        || "project".to_owned(),
        |n| n.to_string_lossy().into_owned(),
    );
    let repository =
        remote_repository(&read("config")).unwrap_or_else(|| format!("local/{folder}"));
    GitInfo {
        git_ref: if git_ref.is_empty() {
            "refs/heads/main".into()
        } else {
            git_ref
        },
        sha: if sha.is_empty() { "0".repeat(40) } else { sha },
        repository,
    }
}

/// `owner/name` of the `origin` remote's URL, SSH or HTTPS.
fn remote_repository(config: &str) -> Option<String> {
    let mut in_origin = false;
    for line in config.lines().map(str::trim) {
        if line.starts_with('[') {
            in_origin = line == "[remote \"origin\"]";
        } else if in_origin && let Some(url) = line.strip_prefix("url") {
            let url = url.trim_start_matches([' ', '=']).trim();
            let path = url
                .rsplit_once(':')
                .filter(|_| !url.contains("://"))
                .map(|(_, p)| p)
                .or_else(|| {
                    url.split("://")
                        .nth(1)
                        .and_then(|r| r.split_once('/').map(|(_, p)| p))
                })?;
            let path = path.trim_end_matches('/').trim_end_matches(".git");
            let mut parts = path.rsplitn(3, '/');
            let (name, owner) = (parts.next()?, parts.next()?);
            return Some(format!("{owner}/{name}"));
        }
    }
    None
}

/// The `github` context, and the event payload (`github.event`, also
/// written to `GITHUB_EVENT_PATH`).
pub fn github(
    info: &GitInfo,
    workflow: &str,
    event: &str,
    inputs: &[(String, String)],
    run: u64,
    actor: &str,
    workspace: &str,
) -> Value {
    let (owner, _) = info.repository.split_once('/').unwrap_or(("local", ""));
    let branch = info
        .git_ref
        .strip_prefix("refs/heads/")
        .unwrap_or(&info.git_ref);
    let repo = json!({
        "full_name": info.repository,
        "name": info.repository.rsplit('/').next(),
        "owner": {"login": owner},
        "default_branch": "main",
    });
    let payload = match event {
        "pull_request" | "pull_request_target" => json!({
            "action": "opened",
            "number": 1,
            "pull_request": {
                "number": 1,
                "head": {"ref": branch, "sha": info.sha},
                "base": {"ref": "main"},
            },
            "repository": repo,
        }),
        "workflow_dispatch" => json!({
            "inputs": inputs.iter().map(|(k, v)| (k.clone(), Value::String(v.clone()))).collect::<serde_json::Map<_, _>>(),
            "ref": info.git_ref,
            "repository": repo,
        }),
        _ => json!({
            "ref": info.git_ref,
            "before": "0".repeat(40),
            "after": info.sha,
            "head_commit": {"id": info.sha},
            "repository": repo,
        }),
    };
    let pull = event.starts_with("pull_request");
    json!({
        "event_name": event,
        "event": payload,
        "ref": if pull { "refs/pull/1/merge".to_owned() } else { info.git_ref.clone() },
        "ref_name": if pull { "1/merge" } else { branch },
        "ref_type": "branch",
        "head_ref": if pull { branch } else { "" },
        "base_ref": if pull { "main" } else { "" },
        "sha": info.sha,
        "repository": info.repository,
        "repository_owner": owner,
        "actor": actor,
        "triggering_actor": actor,
        "workflow": workflow,
        "run_id": run.to_string(),
        "run_number": run.to_string(),
        "run_attempt": "1",
        "server_url": "https://github.com",
        "api_url": "https://api.github.com",
        "graphql_url": "https://api.github.com/graphql",
        "workspace": workspace,
        "action": "",
        "job": "",
        "retention_days": "0",
    })
}

/// The `runner` context.
pub fn runner(temp: &str, tool_cache: &str) -> Value {
    let arch = if cfg!(target_arch = "aarch64") {
        "ARM64"
    } else {
        "X64"
    };
    json!({
        "os": "Linux",
        "arch": arch,
        "name": "Ferrodock Local CI",
        "environment": "self-hosted",
        "temp": temp,
        "tool_cache": tool_cache,
        "debug": "",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_git_and_builds_contexts() {
        let dir = std::env::temp_dir().join(format!("ferrodock-ci-git-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".git/refs/heads")).unwrap();
        std::fs::write(dir.join(".git/HEAD"), "ref: refs/heads/feature\n").unwrap();
        std::fs::write(
            dir.join(".git/packed-refs"),
            "# pack\nabc123 refs/heads/feature\n",
        )
        .unwrap();
        std::fs::write(
            dir.join(".git/config"),
            "[core]\n\tbare = false\n[remote \"origin\"]\n\turl = git@github.com:me/app.git\n",
        )
        .unwrap();
        let info = git_info(&dir);
        assert_eq!(info.git_ref, "refs/heads/feature");
        assert_eq!(info.sha, "abc123");
        assert_eq!(info.repository, "me/app");
        let gh = github(&info, "CI", "push", &[], 7, "mani", "/w");
        assert_eq!(gh["ref_name"], "feature");
        assert_eq!(gh["event"]["after"], "abc123");
        assert_eq!(gh["repository_owner"], "me");
        let pr = github(&info, "CI", "pull_request", &[], 7, "mani", "/w");
        assert_eq!(pr["head_ref"], "feature");
        assert_eq!(pr["event"]["pull_request"]["number"], 1);
        std::fs::remove_dir_all(dir).unwrap();

        assert_eq!(
            remote_repository("[remote \"origin\"]\n url = https://github.com/a/b\n"),
            Some("a/b".into())
        );
        assert_eq!(
            git_info(Path::new("/nonexistent/thing")).repository,
            "local/thing"
        );
    }
}
