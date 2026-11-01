//! Secrets for local runs, kept per project on this computer only: in
//! Ferrodock's folder, in a file only you can read. Never in the project.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// `~/.ferrodock/ci-secrets.json` (on Windows, in local app data).
fn file() -> Option<PathBuf> {
    let dir = if cfg!(windows) {
        PathBuf::from(std::env::var_os("LOCALAPPDATA")?).join("Ferrodock")
    } else {
        PathBuf::from(std::env::var_os("HOME")?).join(".ferrodock")
    };
    Some(dir.join("ci-secrets.json"))
}

type All = BTreeMap<String, Vec<(String, String)>>;

fn read_all(path: &Path) -> All {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// The secrets saved for `project`.
pub fn load(project: &Path) -> Vec<(String, String)> {
    file()
        .map(|f| {
            read_all(&f)
                .remove(&project.display().to_string())
                .unwrap_or_default()
        })
        .unwrap_or_default()
}

/// Saves the secrets of `project` (none removes them).
pub fn save(project: &Path, secrets: &[(String, String)]) -> Result<(), String> {
    let path = file().ok_or("there's no home folder")?;
    save_in(&path, project, secrets)
}

fn save_in(path: &Path, project: &Path, secrets: &[(String, String)]) -> Result<(), String> {
    let mut all = read_all(path);
    let key = project.display().to_string();
    let kept: Vec<(String, String)> = secrets
        .iter()
        .filter(|(k, _)| !k.trim().is_empty())
        .cloned()
        .collect();
    if kept.is_empty() {
        all.remove(&key);
    } else {
        all.insert(key, kept);
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let text = serde_json::to_string_pretty(&all).map_err(|e| e.to_string())?;
    std::fs::write(path, text).map_err(|e| format!("Couldn't save the secrets: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_secrets_per_project() {
        let path =
            std::env::temp_dir().join(format!("ferrodock-ci-secrets-{}.json", std::process::id()));
        let (a, b) = (Path::new("/code/a"), Path::new("/code/b"));
        save_in(
            &path,
            a,
            &[("TOKEN".into(), "x".into()), (" ".into(), "dropped".into())],
        )
        .unwrap();
        save_in(&path, b, &[("KEY".into(), "y".into())]).unwrap();
        let all = read_all(&path);
        assert_eq!(all["/code/a"], [("TOKEN".to_owned(), "x".to_owned())]);
        assert_eq!(all["/code/b"].len(), 1);
        save_in(&path, a, &[]).unwrap();
        assert!(!read_all(&path).contains_key("/code/a"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        std::fs::remove_file(path).unwrap();
    }
}
