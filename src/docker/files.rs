//! Files inside a container: listing a folder, previewing a text file, and
//! copying files out (`docker cp` from) and in (`docker cp` to).
//!
//! Folders are listed with `ls` run inside the container. Containers that
//! aren't running, or have no `ls`, fall back to the archive API, which
//! streams the folder as a tar; only its headers are read.

use std::io::Read;
use std::path::{Path, PathBuf};

use bollard::exec::{CreateExecOptions, StartExecResults};
use bollard::query_parameters::{DownloadFromContainerOptions, UploadToContainerOptions};
use futures_util::StreamExt;

use super::{DockerClient, describe, time_ago};

/// The archive fallback reads at most this much of a folder's tar.
const LIST_CAP: usize = 16 << 20;
/// Bigger files aren't previewed.
pub const PREVIEW_LIMIT: u64 = 512 << 10;

/// Something to do with a container's files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileOp {
    List {
        container: String,
        path: String,
    },
    Preview {
        container: String,
        path: String,
    },
    /// Copy `path` (a file or folder) into the user's Downloads folder.
    Download {
        container: String,
        path: String,
    },
    /// Copy local files or folders into the folder `dir`.
    Upload {
        container: String,
        dir: String,
        files: Vec<PathBuf>,
    },
    /// Get a (helper) container whose files show `volume`.
    BrowseVolume {
        volume: String,
    },
    /// Remove the helper of `volume`.
    EndBrowse {
        volume: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    Dir,
    File,
    Link,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
    pub name: String,
    pub kind: FileKind,
    pub size: u64,
    /// As `ls` shows it ("Oct 17 21:02"), or how long ago.
    pub modified: String,
    /// Where a symbolic link points.
    pub target: Option<String>,
}

/// The contents of one folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listing {
    pub container_id: String,
    pub path: String,
    /// Folders first, then by name.
    pub entries: Vec<FileEntry>,
    /// False when the folder was too big to list completely.
    pub complete: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Preview {
    Text(String),
    Binary,
    TooLarge(u64),
}

/// `dir` + `name` as a container path.
pub fn join_path(dir: &str, name: &str) -> String {
    if dir.ends_with('/') {
        format!("{dir}{name}")
    } else {
        format!("{dir}/{name}")
    }
}

/// The folder above `path`; `/` stays `/`.
pub fn parent_path(path: &str) -> String {
    let trimmed = path.trim_end_matches('/');
    match trimmed.rfind('/') {
        Some(0) | None => "/".into(),
        Some(i) => trimmed[..i].to_owned(),
    }
}

/// Parses `ls -la` output, from GNU coreutils or BusyBox.
fn parse_ls(output: &str) -> Vec<FileEntry> {
    output.lines().filter_map(parse_ls_line).collect()
}

fn parse_ls_line(line: &str) -> Option<FileEntry> {
    let mut rest = line;
    let mut next = || {
        let trimmed = rest.trim_start_matches(' ');
        let end = trimmed.find(' ').unwrap_or(trimmed.len());
        let (token, after) = trimmed.split_at(end);
        rest = after;
        (!token.is_empty()).then_some(token)
    };
    let perms = next()?;
    let kind = match perms.chars().next()? {
        'd' => FileKind::Dir,
        '-' => FileKind::File,
        'l' => FileKind::Link,
        'c' | 'b' | 'p' | 's' => FileKind::Other,
        _ => return None, // "total 12"
    };
    let (_links, _owner, _group) = (next()?, next()?, next()?);
    let mut size = next()?;
    if size.ends_with(',') {
        // A device: "major, minor" instead of a size.
        next()?;
        size = "0";
    }
    let date = [next()?, next()?, next()?].join(" ");
    let name = rest.strip_prefix(' ')?;
    let (name, target) = match (kind, name.split_once(" -> ")) {
        (FileKind::Link, Some((name, target))) => (name, Some(target.to_owned())),
        _ => (name, None),
    };
    if name.is_empty() || name == "." || name == ".." {
        return None;
    }
    Some(FileEntry {
        name: name.to_owned(),
        kind,
        size: size.parse().unwrap_or(0),
        modified: date,
        target,
    })
}

/// Reads the direct children of the folder a tar of it holds. The tar's
/// paths start with the folder's own name, e.g. `etc/hosts`.
fn parse_tar_listing(bytes: &[u8]) -> Vec<FileEntry> {
    let mut entries = Vec::new();
    let mut archive = tar::Archive::new(bytes);
    let Ok(iter) = archive.entries() else {
        return entries;
    };
    // A truncated tar ends in an error; everything before it still counts.
    for entry in iter.map_while(Result::ok) {
        let Ok(path) = entry.path() else { continue };
        let parts: Vec<_> = path.components().collect();
        if parts.len() != 2 {
            continue;
        }
        let header = entry.header();
        let kind = match header.entry_type() {
            t if t.is_dir() => FileKind::Dir,
            t if t.is_file() => FileKind::File,
            t if t.is_symlink() => FileKind::Link,
            _ => FileKind::Other,
        };
        entries.push(FileEntry {
            name: parts[1].as_os_str().to_string_lossy().into_owned(),
            kind,
            size: header.size().unwrap_or(0),
            modified: header
                .mtime()
                .map(|t| time_ago(t as i64))
                .unwrap_or_default(),
            target: header
                .link_name()
                .ok()
                .flatten()
                .map(|t| t.to_string_lossy().into_owned()),
        });
    }
    entries
}

/// Whether the tar of a path holds a folder (its first entry is one).
fn tar_is_folder(bytes: &[u8]) -> bool {
    let mut archive = tar::Archive::new(bytes);
    archive
        .entries()
        .ok()
        .and_then(|mut entries| entries.next())
        .and_then(Result::ok)
        .is_some_and(|entry| entry.header().entry_type().is_dir())
}

fn sort(entries: &mut [FileEntry]) {
    entries.sort_by(|a, b| {
        (a.kind != FileKind::Dir)
            .cmp(&(b.kind != FileKind::Dir))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
}

/// `name`, or `name (1)`, `name (2)`, … if that's taken in `dir`.
fn unique_path(dir: &Path, name: &str) -> PathBuf {
    let first = dir.join(name);
    if !first.exists() {
        return first;
    }
    let (stem, ext) = match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => (stem, format!(".{ext}")),
        _ => (name, String::new()),
    };
    (1..)
        .map(|n| dir.join(format!("{stem} ({n}){ext}")))
        .find(|p| !p.exists())
        .expect("some name is free")
}

/// The user's Downloads folder, or the temp folder if there isn't one.
pub fn downloads_dir() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(|home| PathBuf::from(home).join("Downloads"))
        .filter(|d| d.is_dir())
        .unwrap_or_else(std::env::temp_dir)
}

impl DockerClient {
    pub async fn list_files(&self, id: &str, path: &str) -> Result<Listing, String> {
        let (mut entries, complete) = match self.list_with_ls(id, path).await {
            Ok(entries) => (entries, true),
            Err(ls_error) => {
                let (bytes, complete) =
                    self.archive(id, path, LIST_CAP)
                        .await
                        .map_err(|archive_error| {
                            if archive_error.contains("No such") {
                                archive_error
                            } else {
                                ls_error
                            }
                        })?;
                if !tar_is_folder(&bytes) {
                    return Err(format!("{path} is not a folder"));
                }
                (parse_tar_listing(&bytes), complete)
            }
        };
        sort(&mut entries);
        Ok(Listing {
            container_id: id.to_owned(),
            path: path.to_owned(),
            entries,
            complete,
        })
    }

    /// Runs `ls -la` in the container. The trailing `/` follows a link to a
    /// folder.
    async fn list_with_ls(&self, id: &str, path: &str) -> Result<Vec<FileEntry>, String> {
        let dir = if path.ends_with('/') {
            path.to_owned()
        } else {
            format!("{path}/")
        };
        let exec = self
            .docker
            .create_exec(
                id,
                CreateExecOptions {
                    cmd: Some(vec!["ls".into(), "-la".into(), "--".into(), dir]),
                    env: Some(vec!["LC_ALL=C".into()]),
                    attach_stdout: Some(true),
                    attach_stderr: Some(true),
                    ..Default::default()
                },
            )
            .await
            .map_err(describe)?;
        let StartExecResults::Attached { mut output, .. } = self
            .docker
            .start_exec(&exec.id, None)
            .await
            .map_err(describe)?
        else {
            return Err("ls didn't attach".into());
        };
        let (mut stdout, mut stderr) = (String::new(), String::new());
        while let Some(chunk) = output.next().await {
            match chunk.map_err(describe)? {
                bollard::container::LogOutput::StdOut { message } => {
                    stdout.push_str(&String::from_utf8_lossy(&message));
                }
                bollard::container::LogOutput::StdErr { message } => {
                    stderr.push_str(&String::from_utf8_lossy(&message));
                }
                _ => {}
            }
        }
        let code = self
            .docker
            .inspect_exec(&exec.id)
            .await
            .map_err(describe)?
            .exit_code;
        match code {
            Some(0) => Ok(parse_ls(&stdout)),
            _ => Err(stderr
                .trim()
                .trim_start_matches("ls: ")
                .to_owned()
                .replace("\n", " ")),
        }
    }

    /// Downloads the tar of `path`, stopping after `cap` bytes. Returns the
    /// bytes and whether that was all of it.
    async fn archive(&self, id: &str, path: &str, cap: usize) -> Result<(Vec<u8>, bool), String> {
        let options = DownloadFromContainerOptions {
            path: path.to_owned(),
        };
        let mut stream = self.docker.download_from_container(id, Some(options));
        let mut bytes = Vec::new();
        while let Some(chunk) = stream.next().await {
            bytes.extend_from_slice(&chunk.map_err(describe)?);
            if bytes.len() >= cap {
                return Ok((bytes, false));
            }
        }
        Ok((bytes, true))
    }

    pub async fn preview_file(&self, id: &str, path: &str) -> Result<Preview, String> {
        // Room for the tar header and padding around the file.
        let cap = PREVIEW_LIMIT as usize + 64 * 1024;
        let (bytes, _) = self.archive(id, path, cap).await?;
        let mut archive = tar::Archive::new(bytes.as_slice());
        let mut entry = archive
            .entries()
            .map_err(|e| e.to_string())?
            .next()
            .ok_or("The file is empty")?
            .map_err(|e| e.to_string())?;
        let size = entry.header().size().unwrap_or(0);
        if size > PREVIEW_LIMIT {
            return Ok(Preview::TooLarge(size));
        }
        let mut content = Vec::new();
        entry.read_to_end(&mut content).map_err(|e| e.to_string())?;
        if content.iter().take(8192).any(|b| *b == 0) {
            return Ok(Preview::Binary);
        }
        Ok(Preview::Text(
            String::from_utf8_lossy(&content).into_owned(),
        ))
    }

    /// Copies `path` out of the container into `dest`, under a name that
    /// doesn't overwrite anything. Returns where it went.
    pub async fn download_file(
        &self,
        id: &str,
        path: &str,
        dest: &Path,
    ) -> Result<PathBuf, String> {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        let staging = dest.join(format!(".ferrodock-download-{stamp}"));
        std::fs::create_dir_all(&staging).map_err(|e| e.to_string())?;
        let result = self.download_into(id, path, &staging, dest).await;
        let _ = std::fs::remove_dir_all(&staging);
        result
    }

    async fn download_into(
        &self,
        id: &str,
        path: &str,
        staging: &Path,
        dest: &Path,
    ) -> Result<PathBuf, String> {
        let options = DownloadFromContainerOptions {
            path: path.to_owned(),
        };
        let tar_path = staging.join("archive.tar");
        let mut file = std::fs::File::create(&tar_path).map_err(|e| e.to_string())?;
        let mut stream = self.docker.download_from_container(id, Some(options));
        while let Some(chunk) = stream.next().await {
            std::io::Write::write_all(&mut file, &chunk.map_err(describe)?)
                .map_err(|e| e.to_string())?;
        }
        drop(file);
        let unpacked = staging.join("unpacked");
        let (tar_path, unpacked_dir, dest) = (tar_path.clone(), unpacked.clone(), dest.to_owned());
        tokio::task::spawn_blocking(move || -> Result<PathBuf, String> {
            let file = std::fs::File::open(&tar_path).map_err(|e| e.to_string())?;
            // `unpack` refuses paths that would land outside the folder.
            tar::Archive::new(file)
                .unpack(&unpacked_dir)
                .map_err(|e| e.to_string())?;
            let top = std::fs::read_dir(&unpacked_dir)
                .map_err(|e| e.to_string())?
                .next()
                .ok_or("The container sent nothing")?
                .map_err(|e| e.to_string())?;
            let name = top.file_name().to_string_lossy().into_owned();
            let target = unique_path(&dest, &name);
            std::fs::rename(top.path(), &target).map_err(|e| e.to_string())?;
            Ok(target)
        })
        .await
        .map_err(|e| e.to_string())?
    }

    /// Copies local files and folders into the folder `dir` of the
    /// container.
    pub async fn upload_files(&self, id: &str, dir: &str, files: &[PathBuf]) -> Result<(), String> {
        let files = files.to_vec();
        let tar = tokio::task::spawn_blocking(move || -> Result<Vec<u8>, String> {
            let mut builder = tar::Builder::new(Vec::new());
            for file in &files {
                let name = file
                    .file_name()
                    .ok_or_else(|| format!("{} has no name", file.display()))?;
                let added = if file.is_dir() {
                    builder.append_dir_all(name, file)
                } else {
                    builder.append_path_with_name(file, name)
                };
                added.map_err(|e| format!("Couldn't read {}: {e}", file.display()))?;
            }
            builder.into_inner().map_err(|e| e.to_string())
        })
        .await
        .map_err(|e| e.to_string())??;
        let options = UploadToContainerOptions {
            path: dir.to_owned(),
            ..Default::default()
        };
        self.docker
            .upload_to_container(id, Some(options), bollard::body_full(tar.into()))
            .await
            .map_err(describe)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_gnu_and_busybox_ls() {
        let gnu = "total 12\n\
            drwxr-xr-x 2 root root 4096 Oct 17 21:02 .\n\
            drwxr-xr-x 1 root root 4096 Oct 17 21:02 ..\n\
            -rw-r--r-- 1 root root  612 Aug 11  2025 my notes.txt\n\
            lrwxrwxrwx 1 root root    7 Oct  1 12:00 bin -> usr/bin\n\
            crw-rw-rw- 1 root root 1, 3 Oct 17 21:02 null\n";
        let entries = parse_ls(gnu);
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].name, "my notes.txt");
        assert_eq!(entries[0].size, 612);
        assert_eq!(entries[0].modified, "Aug 11 2025");
        assert_eq!(entries[1].kind, FileKind::Link);
        assert_eq!(entries[1].target.as_deref(), Some("usr/bin"));
        assert_eq!(
            (entries[2].name.as_str(), entries[2].kind),
            ("null", FileKind::Other)
        );

        let busybox = "drwxr-xr-x    2 root     root          4096 Oct 17 21:02 conf.d\n";
        let entries = parse_ls(busybox);
        assert_eq!(entries[0].name, "conf.d");
        assert_eq!(entries[0].kind, FileKind::Dir);
    }

    #[test]
    fn reads_a_folder_from_its_tar() {
        let mut builder = tar::Builder::new(Vec::new());
        let mut add = |path: &str, data: &[u8]| {
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder.append_data(&mut header, path, data).unwrap();
        };
        add("etc/hosts", b"127.0.0.1 localhost\n");
        add("etc/ssl/cert.pem", b"x");
        let bytes = builder.into_inner().unwrap();
        // Without an entry for the folder itself it's not a folder.
        assert!(!tar_is_folder(&bytes));
        let entries = parse_tar_listing(&bytes);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "hosts");
        assert_eq!(entries[0].size, 20);
    }

    #[test]
    fn joins_paths() {
        assert_eq!(join_path("/", "etc"), "/etc");
        assert_eq!(join_path("/etc", "hosts"), "/etc/hosts");
        assert_eq!(parent_path("/etc/nginx"), "/etc");
        assert_eq!(parent_path("/etc"), "/");
        assert_eq!(parent_path("/"), "/");
    }

    #[test]
    fn never_overwrites_downloads() {
        let dir = std::env::temp_dir().join(format!("ferrodock-unique-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.conf"), "").unwrap();
        assert_eq!(unique_path(&dir, "a.conf"), dir.join("a (1).conf"));
        assert_eq!(unique_path(&dir, "b"), dir.join("b"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Talks to a real daemon: `cargo test -- --ignored`. Runs a container,
    /// uploads a file into it, lists, previews and downloads it again, and
    /// lists a folder of the stopped container through the archive API.
    #[tokio::test]
    #[ignore]
    async fn live_daemon_copies_files_in_and_out() {
        let client = DockerClient::connect().unwrap();
        let Some(image) = client.list_images().await.unwrap().into_iter().next() else {
            return;
        };
        let spec = crate::docker::RunSpec {
            image: image.id,
            name: "ferrodock-files".into(),
            command: "sleep 300".into(),
            ..Default::default()
        };
        let opts = || bollard::query_parameters::RemoveContainerOptions {
            force: true,
            ..Default::default()
        };
        // Left over from a run that panicked.
        let _ = client
            .docker
            .remove_container(&spec.name, Some(opts()))
            .await;
        let id = client.run(&spec).await.unwrap();
        let result = copy_in_and_out(&client, &id).await;
        client
            .docker
            .remove_container(&id, Some(opts()))
            .await
            .unwrap();
        result.unwrap();
    }

    async fn copy_in_and_out(client: &DockerClient, id: &str) -> Result<(), String> {
        let work = std::env::temp_dir().join(format!("ferrodock-files-{}", std::process::id()));
        std::fs::create_dir_all(&work).unwrap();
        let local = work.join("ferrodock-test.txt");
        std::fs::write(&local, "hello from ferrodock\n").unwrap();

        client.upload_files(id, "/etc", &[local]).await.unwrap();
        let listing = client.list_files(id, "/etc").await.unwrap();
        assert!(
            listing
                .entries
                .iter()
                .any(|e| e.name == "ferrodock-test.txt")
        );
        let preview = client
            .preview_file(id, "/etc/ferrodock-test.txt")
            .await
            .unwrap();
        assert_eq!(preview, Preview::Text("hello from ferrodock\n".into()));
        let saved = client
            .download_file(id, "/etc/ferrodock-test.txt", &work)
            .await
            .unwrap();
        assert_eq!(saved, work.join("ferrodock-test (1).txt"));
        assert_eq!(
            std::fs::read_to_string(&saved).unwrap(),
            "hello from ferrodock\n"
        );
        // A folder comes out whole.
        let folder = client.download_file(id, "/etc", &work).await.unwrap();
        assert!(folder.join("hostname").exists() || folder.join("hosts").exists());
        std::fs::remove_dir_all(&work).unwrap();

        // Stopped: no exec, so the archive API lists the folder.
        client.perform(crate::docker::Action::Stop, id).await?;
        let listing = client.list_files(id, "/etc").await?;
        assert!(
            listing
                .entries
                .iter()
                .any(|e| e.name == "ferrodock-test.txt")
        );
        Ok(())
    }
}
