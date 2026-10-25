//! Building an image from a Dockerfile, through the Engine's own (classic)
//! builder: no Docker CLI or BuildKit needed. The build context is sent
//! as a tar, without what `.dockerignore` excludes.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use bollard::query_parameters::{BuildImageOptions, BuilderVersion};
use futures_util::StreamExt;

use super::{DockerClient, describe};

/// What to build.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BuildSpec {
    /// Matches the output to the build that printed it.
    pub job: u64,
    /// The build context: the folder the Dockerfile's `COPY`s read from.
    pub context: PathBuf,
    /// The Dockerfile, relative to the context.
    pub dockerfile: String,
    /// `name[:tag]` for the result.
    pub tag: String,
    /// `--build-arg`s.
    pub args: Vec<(String, String)>,
    /// The stage to stop at (`--target`); empty builds the last one.
    pub target: String,
    pub no_cache: bool,
}

impl DockerClient {
    /// Builds `spec`, handing each line of build output to `line`.
    pub async fn build_image(
        &self,
        spec: &BuildSpec,
        mut line: impl FnMut(String),
    ) -> Result<(), String> {
        let (context, dockerfile) = (spec.context.clone(), spec.dockerfile.clone());
        let tar = tokio::task::spawn_blocking(move || context_tar(&context, &dockerfile))
            .await
            .map_err(|e| e.to_string())??;
        line(format!(
            "Sending the build context ({})",
            super::format_size(tar.len() as i64)
        ));
        let options = BuildImageOptions {
            dockerfile: spec.dockerfile.replace('\\', "/"),
            t: Some(spec.tag.clone()),
            buildargs: Some(spec.args.iter().cloned().collect::<HashMap<_, _>>()),
            target: spec.target.clone(),
            nocache: spec.no_cache,
            rm: true,
            forcerm: true,
            version: BuilderVersion::BuilderV1,
            ..Default::default()
        };
        let mut stream =
            self.docker
                .build_image(options, None, Some(bollard::body_full(tar.into())));
        // Output comes in pieces that don't respect line ends.
        let mut pending = String::new();
        while let Some(item) = stream.next().await {
            let info = item.map_err(describe)?;
            if let Some(error) = info.error_detail.and_then(|e| e.message) {
                return Err(error);
            }
            if let Some(text) = info.stream {
                pending.push_str(&text);
                while let Some(end) = pending.find('\n') {
                    let text: String = pending.drain(..=end).collect();
                    line(text.trim_end().to_owned());
                }
            }
            if let Some(status) = info.status {
                line(status);
            }
        }
        if !pending.trim().is_empty() {
            line(pending.trim_end().to_owned());
        }
        Ok(())
    }
}

/// The build context as a tar: everything in `dir` except what its
/// `.dockerignore` excludes. The Dockerfile and `.dockerignore` always go,
/// like the Docker CLI sends them.
pub fn context_tar(dir: &Path, dockerfile: &str) -> Result<Vec<u8>, String> {
    if !dir.is_dir() {
        return Err(format!("{} isn't a folder", dir.display()));
    }
    let dockerfile = dockerfile.replace('\\', "/");
    if !dir.join(&dockerfile).is_file() {
        return Err(format!("there's no {dockerfile} in {}", dir.display()));
    }
    let ignore = std::fs::read_to_string(dir.join(".dockerignore"))
        .map(|text| Ignore::parse(&text))
        .unwrap_or_default();
    let mut tar = tar::Builder::new(Vec::new());
    tar.follow_symlinks(false);
    let keep = |path: &str| path == dockerfile || path == ".dockerignore" || !ignore.excludes(path);
    add_dir(&mut tar, dir, "", &ignore, &keep)?;
    tar.into_inner().map_err(|e| e.to_string())
}

fn add_dir(
    tar: &mut tar::Builder<Vec<u8>>,
    dir: &Path,
    prefix: &str,
    ignore: &Ignore,
    keep: &dyn Fn(&str) -> bool,
) -> Result<(), String> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .map_err(|e| format!("Couldn't read {}: {e}", dir.display()))?
        .filter_map(Result::ok)
        .collect();
    // The same tar for the same folder, so the build cache hits.
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = format!("{prefix}{name}");
        let file_type = entry.file_type().map_err(|e| e.to_string())?;
        let included = keep(&path);
        if file_type.is_dir() {
            // An excluded folder can still hold files a `!` line brings back.
            if !included && !ignore.may_include_below(&path) {
                continue;
            }
            if included {
                tar.append_dir(&path, entry.path())
                    .map_err(|e| e.to_string())?;
            }
            add_dir(tar, &entry.path(), &format!("{path}/"), ignore, keep)?;
        } else if included {
            tar.append_path_with_name(entry.path(), &path)
                .map_err(|e| format!("Couldn't read {path}: {e}"))?;
        }
    }
    Ok(())
}

/// The rules in a `.dockerignore`, as the Docker CLI reads them: one
/// pattern per line, `#` comments, `!` to bring back what an earlier line
/// excluded, and the last matching line wins.
#[derive(Debug, Default)]
pub struct Ignore {
    /// (pattern split into path parts, is an exception).
    rules: Vec<(Vec<String>, bool)>,
}

impl Ignore {
    pub fn parse(text: &str) -> Self {
        let rules = text
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .filter_map(|line| {
                let (pattern, exception) = match line.strip_prefix('!') {
                    Some(rest) => (rest.trim(), true),
                    None => (line, false),
                };
                let parts: Vec<String> = pattern
                    .replace('\\', "/")
                    .split('/')
                    .filter(|p| !p.is_empty() && *p != ".")
                    .map(str::to_owned)
                    .collect();
                (!parts.is_empty()).then_some((parts, exception))
            })
            .collect();
        Self { rules }
    }

    /// Whether `path` (relative, `/`-separated) is left out of the context.
    /// A path is matched by a rule that matches it or a folder it's in.
    pub fn excludes(&self, path: &str) -> bool {
        let parts: Vec<&str> = path.split('/').collect();
        let mut excluded = false;
        for (pattern, exception) in &self.rules {
            let hit = (1..=parts.len()).any(|n| matches(pattern, &parts[..n]));
            if hit {
                excluded = !exception;
            }
        }
        excluded
    }

    /// Whether some `!` line could bring back something inside the folder
    /// `dir`, so it has to be looked into even though it's excluded.
    fn may_include_below(&self, dir: &str) -> bool {
        let depth = dir.split('/').count();
        self.rules.iter().any(|(pattern, exception)| {
            *exception
                && (pattern.iter().any(|p| p == "**")
                    || (pattern.len() > depth
                        && matches(&pattern[..depth], &dir.split('/').collect::<Vec<_>>())))
        })
    }
}

/// Whether the path parts match the pattern parts; `**` matches any
/// number of parts.
fn matches(pattern: &[String], path: &[&str]) -> bool {
    match pattern.split_first() {
        None => path.is_empty(),
        Some((first, rest)) if first == "**" => {
            (0..=path.len()).any(|skip| matches(rest, &path[skip..]))
        }
        Some((first, rest)) => match path.split_first() {
            Some((part, more)) => {
                wildcard(first.as_bytes(), part.as_bytes()) && matches(rest, more)
            }
            None => false,
        },
    }
}

/// One path part against a pattern with `*`, `?`, `[a-z]` / `[^a-z]` and
/// `\` escapes, like Go's `filepath.Match`.
fn wildcard(pattern: &[u8], text: &[u8]) -> bool {
    match pattern.split_first() {
        None => text.is_empty(),
        Some((b'*', rest)) => (0..=text.len()).any(|skip| wildcard(rest, &text[skip..])),
        Some((b'?', rest)) => !text.is_empty() && wildcard(rest, &text[1..]),
        Some((b'[', rest)) => {
            let Some((&c, more)) = text.split_first() else {
                return false;
            };
            let Some(end) = rest.iter().position(|&b| b == b']') else {
                return false;
            };
            let (class, after) = (&rest[..end], &rest[end + 1..]);
            let (negated, class) = match class.split_first() {
                Some((b'^', class)) => (true, class),
                _ => (false, class),
            };
            let mut hit = false;
            let mut i = 0;
            while i < class.len() {
                if i + 2 < class.len() && class[i + 1] == b'-' {
                    hit |= (class[i]..=class[i + 2]).contains(&c);
                    i += 3;
                } else {
                    hit |= class[i] == c;
                    i += 1;
                }
            }
            hit != negated && wildcard(after, more)
        }
        Some((b'\\', rest)) => match (rest.split_first(), text.split_first()) {
            (Some((p, rest)), Some((t, more))) => p == t && wildcard(rest, more),
            _ => false,
        },
        Some((p, rest)) => text.first() == Some(p) && wildcard(rest, &text[1..]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_dockerignore_like_docker() {
        let ignore = Ignore::parse(
            "# comment\n\
             node_modules\n\
             *.log\n\
             /tmp\n\
             **/secret.txt\n\
             docs/*.md\n\
             !docs/README.md\n\
             build?\n\
             [ab].bin\n",
        );
        for excluded in [
            "node_modules",
            "node_modules/x/y.js",
            "app.log",
            "tmp/a",
            "secret.txt",
            "a/b/secret.txt",
            "docs/guide.md",
            "build1",
            "a.bin",
        ] {
            assert!(ignore.excludes(excluded), "{excluded}");
        }
        for kept in [
            "src/app.js",
            "logs/app.txt",
            "sub/app.log",
            "docs/README.md",
            "docs/deep/guide.md",
            "build10",
            "c.bin",
            "Dockerfile",
        ] {
            assert!(!ignore.excludes(kept), "{kept}");
        }
    }

    #[test]
    fn the_last_matching_line_wins() {
        let ignore = Ignore::parse("*\n!src\nsrc/*.tmp\n");
        assert!(ignore.excludes("README.md"));
        assert!(!ignore.excludes("src/main.rs"));
        assert!(ignore.excludes("src/x.tmp"));
        // A folder that's excluded but holds a file a `!` line brings back.
        let ignore = Ignore::parse("src\n!src/keep.txt\n");
        assert!(ignore.excludes("src/other.txt"));
        assert!(!ignore.excludes("src/keep.txt"));
        assert!(ignore.may_include_below("src"));
        assert!(!ignore.may_include_below("target"));
    }

    #[test]
    fn packs_the_context_without_what_is_ignored() {
        let dir = std::env::temp_dir().join(format!("ferrodock-context-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for (path, contents) in [
            ("Dockerfile", "FROM scratch\n"),
            (".dockerignore", "*\n!src\n!Dockerfile\nsrc/*.tmp\n"),
            ("README.md", "readme"),
            ("src/main.rs", "fn main() {}"),
            ("src/scratch.tmp", "junk"),
            ("target/big.bin", "junk"),
        ] {
            let path = dir.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, contents).unwrap();
        }
        let tar = context_tar(&dir, "Dockerfile").unwrap();
        let mut names: Vec<String> = tar::Archive::new(tar.as_slice())
            .entries()
            .unwrap()
            .map(|e| e.unwrap().path().unwrap().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(names, [".dockerignore", "Dockerfile", "src", "src/main.rs"]);
        assert!(context_tar(&dir, "Missing.Dockerfile").is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// Talks to a real daemon: `cargo test -- --ignored`. Builds a tiny
    /// image with a build argument and removes it.
    #[tokio::test]
    #[ignore]
    async fn live_daemon_builds_an_image() {
        let client = DockerClient::connect().unwrap();
        if client
            .list_images()
            .await
            .unwrap()
            .iter()
            .all(|i| !i.tags.iter().any(|t| t == "busybox:latest"))
        {
            return;
        }
        let dir = std::env::temp_dir().join(format!("ferrodock-build-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("Dockerfile"),
            "FROM busybox\nARG GREETING=hi\nRUN echo \"says $GREETING\"\n",
        )
        .unwrap();
        let spec = BuildSpec {
            context: dir.clone(),
            dockerfile: "Dockerfile".into(),
            tag: "ferrodock-build-test:latest".into(),
            args: vec![("GREETING".into(), "hello".into())],
            ..Default::default()
        };
        let mut lines = Vec::new();
        client.build_image(&spec, |l| lines.push(l)).await.unwrap();
        assert!(lines.iter().any(|l| l == "says hello"), "{lines:#?}");
        assert!(lines.iter().any(|l| l.starts_with("Successfully tagged")));
        // A broken Dockerfile is an error, with Docker's reason.
        std::fs::write(dir.join("Dockerfile"), "FROM busybox\nRUN exit 3\n").unwrap();
        let error = client.build_image(&spec, |_| {}).await.unwrap_err();
        assert!(error.contains("non-zero code: 3"), "{error}");
        let _ = client
            .docker
            .remove_image(
                "ferrodock-build-test:latest",
                None::<bollard::query_parameters::RemoveImageOptions>,
                None,
            )
            .await;
        std::fs::remove_dir_all(dir).unwrap();
    }
}
