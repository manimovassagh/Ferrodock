//! The kubeconfig file `kubectl` reads: adding a cluster's context to it
//! (and switching to it), and taking it out again. Everything else in the
//! file is kept; the file as it was is saved next to it first.

use std::path::{Path, PathBuf};

use serde_norway::{Mapping, Value};

/// The file `kubectl` uses: the first in `KUBECONFIG`, else
/// `~/.kube/config`.
pub fn kubeconfig_path() -> Option<PathBuf> {
    if let Some(paths) = std::env::var_os("KUBECONFIG")
        && let Some(first) = std::env::split_paths(&paths).find(|p| !p.as_os_str().is_empty())
    {
        return Some(first);
    }
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    Some(Path::new(&home).join(".kube").join("config"))
}

/// The kubeconfig k3s wrote, with its cluster, user and context all named
/// `context` and the API server at `server`.
pub fn adapt(k3s: &str, context: &str, server: &str) -> Result<Value, String> {
    let mut config: Value = serde_norway::from_str(k3s)
        .map_err(|e| format!("k3s wrote a kubeconfig Ferrodock can't read: {e}"))?;
    for key in ["clusters", "users", "contexts"] {
        let Some(list) = config.get_mut(key).and_then(Value::as_sequence_mut) else {
            return Err(format!("k3s's kubeconfig has no {key}"));
        };
        list.truncate(1);
        let Some(entry) = list.first_mut().and_then(Value::as_mapping_mut) else {
            return Err(format!("k3s's kubeconfig has no {key}"));
        };
        entry.insert("name".into(), context.into());
        if key == "clusters"
            && let Some(cluster) = entry.get_mut("cluster").and_then(Value::as_mapping_mut)
        {
            cluster.insert("server".into(), server.into());
        }
        if key == "contexts"
            && let Some(ctx) = entry.get_mut("context").and_then(Value::as_mapping_mut)
        {
            ctx.insert("cluster".into(), context.into());
            ctx.insert("user".into(), context.into());
        }
    }
    if let Some(map) = config.as_mapping_mut() {
        map.insert("current-context".into(), context.into());
    }
    Ok(config)
}

/// `existing` (a kubeconfig, maybe empty) with the cluster, user and context
/// of `new` added, replacing ones of the same name, and switched to.
pub fn merge(existing: &str, new: &Value) -> Result<String, String> {
    let mut config = parse(existing)?;
    let map = config
        .as_mapping_mut()
        .ok_or("the kubeconfig isn't a YAML map")?;
    for key in ["clusters", "users", "contexts"] {
        let added = new
            .get(key)
            .and_then(Value::as_sequence)
            .cloned()
            .unwrap_or_default();
        let names: Vec<&Value> = added.iter().filter_map(|e| e.get("name")).collect();
        let list = list(map, key);
        list.retain(|e| e.get("name").is_none_or(|n| !names.contains(&n)));
        list.extend(added.iter().cloned());
    }
    if let Some(current) = new.get("current-context") {
        map.insert("current-context".into(), current.clone());
    }
    serde_norway::to_string(&config).map_err(|e| e.to_string())
}

/// `existing` without the cluster, user and context named `context`.
/// Points nowhere if it was the current one.
pub fn remove(existing: &str, context: &str) -> Result<String, String> {
    let mut config = parse(existing)?;
    let map = config
        .as_mapping_mut()
        .ok_or("the kubeconfig isn't a YAML map")?;
    for key in ["clusters", "users", "contexts"] {
        list(map, key).retain(|e| e.get("name").and_then(Value::as_str) != Some(context));
    }
    if map.get("current-context").and_then(Value::as_str) == Some(context) {
        map.remove("current-context");
    }
    serde_norway::to_string(&config).map_err(|e| e.to_string())
}

fn parse(text: &str) -> Result<Value, String> {
    let mut config: Value = if text.trim().is_empty() {
        Value::Mapping(Mapping::new())
    } else {
        serde_norway::from_str(text)
            .map_err(|e| format!("your kubeconfig isn't valid YAML: {e}"))?
    };
    if let Some(map) = config.as_mapping_mut() {
        map.entry("apiVersion".into()).or_insert("v1".into());
        map.entry("kind".into()).or_insert("Config".into());
    }
    Ok(config)
}

/// The list under `key`, made if missing.
fn list<'a>(map: &'a mut Mapping, key: &str) -> &'a mut Vec<Value> {
    let entry = map.entry(key.into()).or_insert(Value::Sequence(Vec::new()));
    if !entry.is_sequence() {
        *entry = Value::Sequence(Vec::new());
    }
    entry.as_sequence_mut().expect("just made a sequence")
}

/// Rewrites the kubeconfig at `path` with `change`, keeping the file as it
/// was in `<path>.ferrodock-backup`. Only you can read the new file.
pub fn update(
    path: &Path,
    change: impl FnOnce(&str) -> Result<String, String>,
) -> Result<(), String> {
    let existing = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(format!("Couldn't read {}: {e}", path.display())),
    };
    let updated = change(&existing)?;
    let dir = path.parent().ok_or("the kubeconfig has no folder")?;
    std::fs::create_dir_all(dir).map_err(|e| format!("Couldn't make {}: {e}", dir.display()))?;
    if !existing.is_empty() {
        let backup = path.with_file_name(format!(
            "{}.ferrodock-backup",
            path.file_name()
                .map_or("config".into(), |n| n.to_string_lossy())
        ));
        std::fs::write(&backup, &existing)
            .map_err(|e| format!("Couldn't back up {}: {e}", path.display()))?;
    }
    let temp = path.with_extension("ferrodock-new");
    std::fs::write(&temp, updated)
        .map_err(|e| format!("Couldn't write {}: {e}", temp.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&temp, std::fs::Permissions::from_mode(0o600));
    }
    std::fs::rename(&temp, path).map_err(|e| format!("Couldn't write {}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const K3S: &str = "apiVersion: v1
clusters:
- cluster:
    certificate-authority-data: Q0E=
    server: https://127.0.0.1:6443
  name: default
contexts:
- context:
    cluster: default
    user: default
  name: default
current-context: default
kind: Config
preferences: {}
users:
- name: default
  user:
    client-certificate-data: Q0VSVA==
    client-key-data: S0VZ
";

    const MINE: &str = "apiVersion: v1
kind: Config
clusters:
- name: work
  cluster:
    server: https://work.example:6443
contexts:
- name: work
  context:
    cluster: work
    user: me
current-context: work
users:
- name: me
  user:
    token: secret
";

    #[test]
    fn adds_and_removes_a_cluster() {
        let new = adapt(K3S, "ferrodock-dev", "https://127.0.0.1:6550").unwrap();
        let merged = merge(MINE, &new).unwrap();
        let config: Value = serde_norway::from_str(&merged).unwrap();
        assert_eq!(config["current-context"], "ferrodock-dev");
        let clusters = config["clusters"].as_sequence().unwrap();
        assert_eq!(clusters.len(), 2);
        assert_eq!(clusters[1]["cluster"]["server"], "https://127.0.0.1:6550");
        assert_eq!(clusters[1]["cluster"]["certificate-authority-data"], "Q0E=");
        assert_eq!(config["contexts"][1]["context"]["user"], "ferrodock-dev");
        assert_eq!(config["users"][0]["user"]["token"], "secret");

        // Again (a re-created cluster): replaced, not added twice.
        let again = merge(&merged, &new).unwrap();
        let config: Value = serde_norway::from_str(&again).unwrap();
        assert_eq!(config["clusters"].as_sequence().unwrap().len(), 2);

        let removed = remove(&again, "ferrodock-dev").unwrap();
        let config: Value = serde_norway::from_str(&removed).unwrap();
        assert_eq!(config["clusters"].as_sequence().unwrap().len(), 1);
        assert_eq!(config["contexts"][0]["name"], "work");
        assert!(config.get("current-context").is_none());
    }

    #[test]
    fn starts_a_missing_kubeconfig() {
        let new = adapt(K3S, "ferrodock-a", "https://localhost:6550").unwrap();
        let config: Value = serde_norway::from_str(&merge("", &new).unwrap()).unwrap();
        assert_eq!(config["kind"], "Config");
        assert_eq!(config["contexts"][0]["name"], "ferrodock-a");
        assert!(merge("- not a map", &new).is_err());
    }

    #[test]
    fn rewrites_the_file_and_keeps_a_backup() {
        let dir = std::env::temp_dir().join(format!("ferrodock-kubeconfig-{}", std::process::id()));
        let path = dir.join("config");
        let new = adapt(K3S, "ferrodock-dev", "https://127.0.0.1:6550").unwrap();
        update(&path, |old| merge(old, &new)).unwrap();
        assert!(
            std::fs::read_to_string(&path)
                .unwrap()
                .contains("ferrodock-dev")
        );
        update(&path, |old| remove(old, "ferrodock-dev")).unwrap();
        assert!(
            !std::fs::read_to_string(&path)
                .unwrap()
                .contains("ferrodock-dev")
        );
        assert!(
            std::fs::read_to_string(dir.join("config.ferrodock-backup"))
                .unwrap()
                .contains("ferrodock-dev")
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}
