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

    /// A fresh, empty directory for one test.
    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ferrodock-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Serves `files` (path, body) over plain HTTP on localhost, one
    /// request per connection, until the test ends.
    fn serve(files: Vec<(&'static str, Vec<u8>)>) -> String {
        use std::io::{BufRead, BufReader, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut request = String::new();
                let _ = reader.read_line(&mut request);
                // The rest of the headers, up to the blank line.
                let mut line = String::new();
                while reader.read_line(&mut line).is_ok_and(|n| n > 2) {
                    line.clear();
                }
                let path = request.split(' ').nth(1).unwrap_or_default();
                let response = match files.iter().find(|(p, _)| *p == path) {
                    Some((_, body)) => {
                        let mut r = format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        )
                        .into_bytes();
                        r.extend_from_slice(body);
                        r
                    }
                    None => {
                        b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                            .to_vec()
                    }
                };
                let _ = stream.write_all(&response);
            }
        });
        base
    }

    /// A `.tar.gz` holding one executable file `name`.
    fn tar_gz(name: &str, contents: &[u8]) -> Vec<u8> {
        let gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        let mut tar = tar::Builder::new(gz);
        let mut header = tar::Header::new_gnu();
        header.set_size(contents.len() as u64);
        header.set_mode(0o755);
        header.set_cksum();
        tar.append_data(&mut header, name, contents).unwrap();
        tar.into_inner().unwrap().finish().unwrap()
    }

    fn available(base: &str, sums: bool) -> Available {
        Available {
            version: "9.9.9".into(),
            notes: String::new(),
            asset_name: "Ferrodock-9.9.9-linux-x64.tar.gz".into(),
            asset_url: format!("{base}/Ferrodock-9.9.9-linux-x64.tar.gz"),
            sums_url: sums.then(|| format!("{base}/SHA256SUMS.txt")),
        }
    }

    #[test]
    fn prepares_a_verified_download() {
        let archive = tar_gz("ferrodock", b"new build");
        let sum = ferrodock_core::hex(&Sha256::digest(&archive));
        let base = serve(vec![
            ("/Ferrodock-9.9.9-linux-x64.tar.gz", archive),
            (
                "/SHA256SUMS.txt",
                format!("{sum}  Ferrodock-9.9.9-linux-x64.tar.gz\n").into_bytes(),
            ),
        ]);
        let work = scratch("prepare-ok");
        let unpacked = prepare(&available(&base, true), |_, _| {}, &work, false).unwrap();
        assert_eq!(fs::read(unpacked.join("ferrodock")).unwrap(), b"new build");
        fs::remove_dir_all(work).unwrap();
    }

    #[test]
    fn refuses_a_corrupted_download() {
        let base = serve(vec![
            (
                "/Ferrodock-9.9.9-linux-x64.tar.gz",
                tar_gz("ferrodock", b"tampered"),
            ),
            (
                "/SHA256SUMS.txt",
                format!("{}  Ferrodock-9.9.9-linux-x64.tar.gz\n", "0".repeat(64)).into_bytes(),
            ),
        ]);
        let work = scratch("prepare-corrupt");
        let error = prepare(&available(&base, true), |_, _| {}, &work, false).unwrap_err();
        assert!(error.contains("checksum mismatch"), "{error}");
        assert!(!work.join("unpacked").exists());
        fs::remove_dir_all(work).unwrap();
    }

    #[test]
    fn refuses_a_download_the_sums_dont_list() {
        let base = serve(vec![
            (
                "/Ferrodock-9.9.9-linux-x64.tar.gz",
                tar_gz("ferrodock", b"new build"),
            ),
            ("/SHA256SUMS.txt", b"abc  something-else.zip\n".to_vec()),
        ]);
        let work = scratch("prepare-unlisted");
        let error = prepare(&available(&base, true), |_, _| {}, &work, false).unwrap_err();
        assert!(error.contains("no checksum"), "{error}");
        fs::remove_dir_all(work).unwrap();
    }

    #[test]
    fn reports_a_failed_download() {
        let base = serve(Vec::new());
        let work = scratch("prepare-404");
        assert!(prepare(&available(&base, true), |_, _| {}, &work, false).is_err());
        fs::remove_dir_all(work).unwrap();
    }

    /// A fake installed `Ferrodock.app` in `dir`, holding `marker`.
    fn fake_app(dir: &Path, marker: &str) -> PathBuf {
        let macos = dir.join("Ferrodock.app/Contents/MacOS");
        fs::create_dir_all(&macos).unwrap();
        fs::write(macos.join("ferrodock"), marker).unwrap();
        macos.join("ferrodock")
    }

    #[test]
    fn swaps_the_app_bundle() {
        let dir = scratch("swap-app");
        let exe = fake_app(&dir.join("Applications"), "old");
        let work = dir.join("work");
        fake_app(&work.join("unpacked"), "new");
        let relaunch = swap(&exe, &work.join("unpacked"), &work).unwrap();
        assert_eq!(
            relaunch,
            Relaunch::App(dir.join("Applications/Ferrodock.app"))
        );
        assert_eq!(fs::read_to_string(&exe).unwrap(), "new");
        // The old one waits in the work folder, which `install` removes.
        assert_eq!(
            fs::read_to_string(work.join("previous.app/Contents/MacOS/ferrodock")).unwrap(),
            "old"
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn keeps_the_app_when_the_download_has_none() {
        let dir = scratch("swap-missing");
        let exe = fake_app(&dir.join("Applications"), "old");
        let work = dir.join("work");
        fs::create_dir_all(work.join("unpacked")).unwrap();
        let error = swap(&exe, &work.join("unpacked"), &work).unwrap_err();
        assert!(error.contains("Ferrodock.app"), "{error}");
        assert_eq!(fs::read_to_string(&exe).unwrap(), "old");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn keeps_the_binary_when_the_download_has_none() {
        let dir = scratch("swap-exe-missing");
        let exe = dir.join("bin/ferrodock");
        fs::create_dir_all(exe.parent().unwrap()).unwrap();
        fs::write(&exe, "old").unwrap();
        fs::create_dir_all(dir.join("work/unpacked")).unwrap();
        let error = swap(&exe, &dir.join("work/unpacked"), &dir.join("work")).unwrap_err();
        assert!(error.contains("doesn't contain"), "{error}");
        assert_eq!(fs::read_to_string(&exe).unwrap(), "old");
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
