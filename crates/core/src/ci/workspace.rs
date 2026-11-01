//! The project's files as a job sees them: what a fresh clone would hold
//! (the `.gitignore`d files left out, `.git` kept), and `hashFiles`.

use std::path::Path;

use sha2::{Digest, Sha256};

use super::super::build::{Ignore, folder_tar};

/// The top-level `.gitignore` as ignore rules. A pattern without a `/`
/// matches at any depth, as in git.
fn gitignore(project: &Path) -> Ignore {
    let text = std::fs::read_to_string(project.join(".gitignore")).unwrap_or_default();
    let rules: Vec<String> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|line| {
            let (bang, pattern) = match line.strip_prefix('!') {
                Some(rest) => ("!", rest),
                None => ("", line),
            };
            let pattern = pattern.trim_end_matches('/');
            if let Some(anchored) = pattern.strip_prefix('/') {
                format!("{bang}{anchored}")
            } else if pattern.contains('/') {
                format!("{bang}{pattern}")
            } else {
                format!("{bang}**/{pattern}")
            }
        })
        .collect();
    Ignore::parse(&rules.join("\n"))
}

/// The project as a tar, for `actions/checkout`.
pub fn project_tar(project: &Path) -> Result<Vec<u8>, String> {
    let ignore = gitignore(project);
    folder_tar(project, &ignore, &|path| {
        path == ".git" || path.starts_with(".git/") || !ignore.excludes(path)
    })
}

/// `hashFiles(patterns)`: a SHA-256 of the project's files that match,
/// in path order; empty when none match.
pub fn hash_files(project: &Path, patterns: &[String]) -> String {
    let wanted = Ignore::parse(&patterns.join("\n"));
    let skipped = gitignore(project);
    let mut files = Vec::new();
    collect(project, "", &wanted, &skipped, &mut files);
    if files.is_empty() {
        return String::new();
    }
    files.sort();
    let mut all = Sha256::new();
    for path in files {
        if let Ok(bytes) = std::fs::read(project.join(&path)) {
            all.update(Sha256::digest(&bytes));
        }
    }
    all.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

fn collect(dir: &Path, prefix: &str, wanted: &Ignore, skipped: &Ignore, files: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = format!("{prefix}{name}");
        if name == ".git" || skipped.excludes(&path) {
            continue;
        }
        match entry.file_type() {
            Ok(t) if t.is_dir() => {
                collect(&entry.path(), &format!("{path}/"), wanted, skipped, files)
            }
            Ok(t) if t.is_file() && wanted.excludes(&path) => files.push(path),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copies_what_a_clone_would_and_hashes_files() {
        let dir = std::env::temp_dir().join(format!("ferrodock-ci-ws-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for (path, text) in [
            (".gitignore", "target/\n*.log\n/secret.txt\n"),
            ("src/main.rs", "fn main() {}"),
            ("app/target/big.bin", "x"),
            ("debug.log", "x"),
            ("secret.txt", "x"),
            ("lib/package-lock.json", "{}"),
            (".git/HEAD", "ref: refs/heads/main\n"),
        ] {
            let file = dir.join(path);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, text).unwrap();
        }
        let tar = project_tar(&dir).unwrap();
        let mut names: Vec<String> = tar::Archive::new(&tar[..])
            .entries()
            .unwrap()
            .map(|e| e.unwrap().path().unwrap().display().to_string())
            .collect();
        names.sort();
        assert!(names.contains(&"src/main.rs".to_owned()), "{names:?}");
        assert!(names.contains(&".git/HEAD".to_owned()), "{names:?}");
        assert!(
            !names
                .iter()
                .any(|n| n.contains("target") || n.ends_with(".log") || n == "secret.txt"),
            "{names:?}"
        );

        let one = hash_files(&dir, &["**/package-lock.json".into()]);
        assert_eq!(one.len(), 64);
        assert_eq!(hash_files(&dir, &["nothing.txt".into()]), "");
        std::fs::write(dir.join("lib/package-lock.json"), "{\"a\":1}").unwrap();
        assert_ne!(hash_files(&dir, &["**/package-lock.json".into()]), one);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
