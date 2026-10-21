//! Downloading, verifying and swapping in a new build.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use self_update::{ArchiveKind, Compression, Download, Extract};
use sha2::{Digest, Sha256};

use super::Available;

/// How to start the new version once it's installed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Relaunch {
    /// Run this executable.
    Exe(PathBuf),
    /// `open` this app bundle (macOS).
    App(PathBuf),
}

impl Relaunch {
    /// Starts the new version, which waits for this one to exit before
    /// it runs. The caller then quits this one (not to the tray).
    pub fn start(&self) -> std::io::Result<()> {
        let flag = crate::instance::AFTER_UPDATE_FLAG;
        match self {
            Relaunch::Exe(path) => std::process::Command::new(path).arg(flag).spawn().map(drop),
            // `-n`: this one still runs, and `open` would only bring it
            // forward.
            Relaunch::App(path) => std::process::Command::new("open")
                .arg("-n")
                .arg(path)
                .args(["--args", flag])
                .spawn()
                .map(drop),
        }
    }
}

/// Downloads `available`, checks its checksum and installs it. `progress`
/// gets (bytes done, total if known).
pub fn install(
    available: &Available,
    progress: impl Fn(u64, Option<u64>) + Send + Sync + 'static,
) -> Result<Relaunch, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe = exe.canonicalize().unwrap_or(exe);
    // Work next to the install, so the final move stays on one disk.
    let base = app_bundle(&exe)
        .and_then(|app| app.parent().map(Path::to_path_buf))
        .or_else(|| exe.parent().map(Path::to_path_buf))
        .ok_or("can't tell where Ferrodock is installed")?;
    let work = tempfile_dir(&base)?;
    let result = install_in(available, progress, &exe, &work);
    let _ = fs::remove_dir_all(&work);
    result
}

fn install_in(
    available: &Available,
    progress: impl Fn(u64, Option<u64>) + Send + Sync + 'static,
    exe: &Path,
    work: &Path,
) -> Result<Relaunch, String> {
    let unpacked = prepare(available, progress, work, app_bundle(exe).is_some())?;
    swap(exe, &unpacked, work)
}

/// Downloads, verifies and unpacks the build into `work/unpacked`.
fn prepare(
    available: &Available,
    progress: impl Fn(u64, Option<u64>) + Send + Sync + 'static,
    work: &Path,
    use_ditto: bool,
) -> Result<PathBuf, String> {
    let archive = work.join(&available.asset_name);
    let file = fs::File::create(&archive).map_err(|e| e.to_string())?;
    Download::from_url(&available.asset_url)
        .request_header("Accept", "application/octet-stream")
        .progress_callback(progress)
        .download_to(file)
        .map_err(|e| e.to_string())?;

    if let Some(sums_url) = &available.sums_url {
        let mut sums = Vec::new();
        Download::from_url(sums_url)
            .request_header("Accept", "application/octet-stream")
            .download_to(&mut sums)
            .map_err(|e| e.to_string())?;
        let sums = String::from_utf8_lossy(&sums);
        let expected = expected_sum(&sums, &available.asset_name)
            .ok_or("the release has no checksum for this download")?;
        let actual = sha256_of(&archive)?;
        if !expected.eq_ignore_ascii_case(&actual) {
            return Err("the download is corrupted (checksum mismatch)".into());
        }
    }

    let unpacked = work.join("unpacked");
    fs::create_dir_all(&unpacked).map_err(|e| e.to_string())?;
    if use_ditto {
        // macOS: `ditto` keeps permissions and signatures intact.
        let status = std::process::Command::new("ditto")
            .args(["-x", "-k"])
            .arg(&archive)
            .arg(&unpacked)
            .status()
            .map_err(|e| e.to_string())?;
        if !status.success() {
            return Err("couldn't unpack the download".into());
        }
    } else {
        let kind = if available.asset_name.ends_with(".tar.gz") {
            ArchiveKind::Tar(Some(Compression::Gz))
        } else {
            ArchiveKind::Zip
        };
        Extract::from_source(&archive)
            .archive(kind)
            .extract_into(&unpacked)
            .map_err(|e| e.to_string())?;
    }
    Ok(unpacked)
}

/// Puts the unpacked build in place of the running one.
fn swap(exe: &Path, unpacked: &Path, work: &Path) -> Result<Relaunch, String> {
    if let Some(app) = app_bundle(exe) {
        let new_app = unpacked.join("Ferrodock.app");
        if !new_app.exists() {
            return Err("the download doesn't contain Ferrodock.app".into());
        }
        let backup = work.join("previous.app");
        fs::rename(&app, &backup).map_err(|e| format!("couldn't replace the app: {e}"))?;
        if let Err(e) = fs::rename(&new_app, &app) {
            let _ = fs::rename(&backup, &app);
            return Err(format!("couldn't replace the app: {e}"));
        }
        return Ok(Relaunch::App(app));
    }

    let name = if cfg!(windows) {
        "Ferrodock.exe"
    } else {
        "ferrodock"
    };
    let new_exe = unpacked.join(name);
    if !new_exe.exists() {
        return Err(format!("the download doesn't contain {name}"));
    }
    self_replace::self_replace(&new_exe).map_err(|e| format!("couldn't replace the app: {e}"))?;
    Ok(Relaunch::Exe(exe.to_path_buf()))
}

/// `…/Ferrodock.app` when running from an app bundle.
fn app_bundle(exe: &Path) -> Option<PathBuf> {
    let app = exe.parent()?.parent()?.parent()?;
    (app.extension().is_some_and(|e| e == "app")).then(|| app.to_path_buf())
}

/// A fresh, empty working directory inside `base`.
fn tempfile_dir(base: &Path) -> Result<PathBuf, String> {
    let dir = base.join(format!(".ferrodock-update-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir)
        .map_err(|e| format!("can't write next to the app ({}): {e}", base.display()))?;
    Ok(dir)
}

/// The checksum `SHA256SUMS.txt` lists for `file`.
fn expected_sum(sums: &str, file: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let (hash, name) = line.split_once(char::is_whitespace)?;
        (name.trim().trim_start_matches('*') == file).then(|| hash.to_owned())
    })
}

fn sha256_of(path: &Path) -> Result<String, String> {
    let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0; 64 * 1024];
    loop {
        let n = file.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(ferrodock_core::hex(&hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_checksums() {
        let sums =
            "abc123  Ferrodock-1.0.0-linux-x64.tar.gz\ndef456 *Ferrodock-1.0.0-windows-x64.zip\n";
        assert_eq!(
            expected_sum(sums, "Ferrodock-1.0.0-linux-x64.tar.gz").as_deref(),
            Some("abc123")
        );
        assert_eq!(
            expected_sum(sums, "Ferrodock-1.0.0-windows-x64.zip").as_deref(),
            Some("def456")
        );
        assert_eq!(expected_sum(sums, "other"), None);
    }

    #[test]
    fn finds_the_app_bundle() {
        let exe = Path::new("/Applications/Ferrodock.app/Contents/MacOS/ferrodock");
        assert_eq!(
            app_bundle(exe),
            Some(PathBuf::from("/Applications/Ferrodock.app"))
        );
        assert_eq!(app_bundle(Path::new("/usr/local/bin/ferrodock")), None);
    }

    #[test]
    fn hashes_files() {
        let dir = std::env::temp_dir().join(format!("ferrodock-hash-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("x");
        fs::write(&file, b"abc").unwrap();
        assert_eq!(
            sha256_of(&file).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        fs::remove_dir_all(dir).unwrap();
    }

    /// Downloads, verifies and unpacks the real latest release from GitHub
    /// (without installing it): `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn live_prepares_the_latest_release() {
        use self_update::backends::github::ReleaseList;
        let suffix = super::super::asset_suffix().unwrap();
        let release = ReleaseList::configure()
            .repo_owner(super::super::OWNER)
            .repo_name(super::super::REPO)
            .build()
            .unwrap()
            .fetch()
            .unwrap()
            .into_vec()
            .into_iter()
            .find(|r| r.assets().iter().any(|a| a.name() == "SHA256SUMS.txt"))
            .unwrap();
        let asset = release
            .assets()
            .iter()
            .find(|a| a.name().ends_with(suffix))
            .unwrap();
        let available = Available {
            version: release.version().into(),
            notes: String::new(),
            asset_name: asset.name().into(),
            asset_url: asset.download_url().into(),
            sums_url: release
                .assets()
                .iter()
                .find(|a| a.name() == "SHA256SUMS.txt")
                .map(|a| a.download_url().into()),
        };
        let work = std::env::temp_dir().join(format!("ferrodock-prepare-{}", std::process::id()));
        fs::create_dir_all(&work).unwrap();
        let unpacked = prepare(&available, |_, _| {}, &work, cfg!(target_os = "macos")).unwrap();
        let name = if cfg!(target_os = "macos") {
            "Ferrodock.app"
        } else if cfg!(windows) {
            "Ferrodock.exe"
        } else {
            "ferrodock"
        };
        assert!(unpacked.join(name).exists());
        fs::remove_dir_all(work).unwrap();
    }
}
