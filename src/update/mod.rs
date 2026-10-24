//! In-app updates from GitHub Releases.
//!
//! [`Updater::check`] looks for a newer release; [`Updater::install`]
//! downloads the build for this platform, checks it against the release's
//! `SHA256SUMS.txt`, and swaps it in place: the executable on Windows and
//! Linux, the whole `Ferrodock.app` on macOS. The app restarts when the
//! user says so. Both run on their own thread and report through
//! [`Status`].

mod install;

use std::sync::{Arc, Mutex};
use std::thread;

use self_update::backends::github::ReleaseList;

pub use install::Relaunch;

const OWNER: &str = "manimovassagh";
const REPO: &str = "Ferrodock";
pub const CURRENT: &str = env!("CARGO_PKG_VERSION");

/// A newer release, and where to get it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Available {
    pub version: String,
    pub notes: String,
    asset_name: String,
    asset_url: String,
    /// Its `SHA256SUMS.txt`. A release without one is never offered: the
    /// download couldn't be checked.
    sums_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Status {
    #[default]
    Idle,
    Checking,
    UpToDate,
    Available(Available),
    Downloading {
        version: String,
        done: u64,
        total: Option<u64>,
    },
    /// Installed; takes effect after a restart.
    Ready {
        version: String,
        relaunch: Relaunch,
    },
    Failed(String),
}

#[derive(Clone)]
pub struct Updater {
    status: Arc<Mutex<Status>>,
    repaint: Arc<dyn Fn() + Send + Sync>,
}

impl Updater {
    /// `repaint` is called whenever the status changes, so the UI can
    /// redraw.
    pub fn new(repaint: impl Fn() + Send + Sync + 'static) -> Self {
        Self {
            status: Arc::new(Mutex::new(Status::Idle)),
            repaint: Arc::new(repaint),
        }
    }

    /// Shows a made-up status, for screenshots.
    #[cfg(test)]
    pub fn pretend(&self, status: Status) {
        self.set(status);
    }

    pub fn status(&self) -> Status {
        self.status.lock().map(|s| s.clone()).unwrap_or_default()
    }

    fn set(&self, status: Status) {
        if let Ok(mut current) = self.status.lock() {
            *current = status;
        }
        (self.repaint)();
    }

    /// Moves to `next` unless a check or download is running, checked and
    /// changed under one lock: two clicks never start two downloads into
    /// the same folder.
    fn claim(&self, next: Status) -> bool {
        let Ok(mut current) = self.status.lock() else {
            return false;
        };
        if matches!(
            *current,
            Status::Checking | Status::Downloading { .. } | Status::Ready { .. }
        ) {
            return false;
        }
        *current = next;
        drop(current);
        (self.repaint)();
        true
    }

    /// Looks for a newer release in the background.
    pub fn check(&self) {
        if !self.claim(Status::Checking) {
            return;
        }
        let this = self.clone();
        thread::spawn(move || {
            let status = match latest() {
                Ok(Some(available)) => Status::Available(available),
                Ok(None) => Status::UpToDate,
                Err(e) => Status::Failed(format!("Couldn't check for updates: {e}")),
            };
            this.set(status);
        });
    }

    /// Downloads and installs `available` in the background.
    pub fn install(&self, available: Available) {
        let starting = Status::Downloading {
            version: available.version.clone(),
            done: 0,
            total: None,
        };
        if !self.claim(starting) {
            return;
        }
        let this = self.clone();
        thread::spawn(move || {
            let version = available.version.clone();
            let progress = {
                let this = this.clone();
                let version = version.clone();
                move |done, total| {
                    this.set(Status::Downloading {
                        version: version.clone(),
                        done,
                        total,
                    })
                }
            };
            progress(0, None);
            let status = match install::install(&available, progress) {
                Ok(relaunch) => Status::Ready { version, relaunch },
                Err(e) => Status::Failed(format!("Couldn't update: {e}")),
            };
            this.set(status);
        });
    }
}

#[cfg(test)]
impl Available {
    pub fn example(version: &str) -> Self {
        Self {
            version: version.into(),
            notes: "### Added\n- In-app updates".into(),
            asset_name: String::new(),
            asset_url: String::new(),
            sums_url: String::new(),
        }
    }
}

/// The build archive's name suffix for this platform.
fn asset_suffix() -> Option<&'static str> {
    if cfg!(target_os = "macos") {
        Some("-macos-universal.zip")
    } else if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        Some("-windows-x64.zip")
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        Some("-linux-x64.tar.gz")
    } else {
        None
    }
}

/// The newest stable release, if it's newer than this build.
fn latest() -> Result<Option<Available>, String> {
    let suffix = asset_suffix().ok_or("there are no prebuilt updates for this platform")?;
    let releases = ReleaseList::configure()
        .repo_owner(OWNER)
        .repo_name(REPO)
        .build()
        .and_then(|list| list.fetch())
        .map_err(|e| e.to_string())?
        .into_vec();
    let newest = releases
        .into_iter()
        // Pre-releases (1.0.0-rc.1) are opt-in elsewhere; never offer them.
        .filter(|r| !r.version().contains('-'))
        // Only releases whose download can be checked: a release still
        // being published, or one missing its checksums, is skipped.
        .find(|r| {
            r.assets().iter().any(|a| a.name().ends_with(suffix))
                && r.assets().iter().any(|a| a.name() == "SHA256SUMS.txt")
        });
    let Some(release) = newest else {
        return Ok(None);
    };
    if !is_newer(CURRENT, release.version()) {
        return Ok(None);
    }
    let asset = release
        .assets()
        .iter()
        .find(|a| a.name().ends_with(suffix))
        .expect("filtered above");
    let sums_url = release
        .assets()
        .iter()
        .find(|a| a.name() == "SHA256SUMS.txt")
        .expect("filtered above")
        .download_url()
        .to_owned();
    Ok(Some(Available {
        version: release.version().to_owned(),
        notes: release.body().unwrap_or_default().to_owned(),
        asset_name: asset.name().to_owned(),
        asset_url: asset.download_url().to_owned(),
        sums_url,
    }))
}

fn is_newer(current: &str, other: &str) -> bool {
    self_update::version::bump_is_greater(current, other).unwrap_or(false)
}

/// Whether Ferrodock was installed by Homebrew, in which case `brew upgrade`
/// is the better way to update (it keeps brew's records right).
pub fn installed_with_homebrew() -> bool {
    cfg!(target_os = "macos")
        && [
            "/opt/homebrew/Caskroom/ferrodock",
            "/usr/local/Caskroom/ferrodock",
        ]
        .iter()
        .any(|p| std::path::Path::new(p).exists())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_one_check_or_download_at_a_time() {
        let updater = Updater::new(|| {});
        assert!(updater.claim(Status::Checking));
        assert!(!updater.claim(Status::Checking));
        let downloading = Status::Downloading {
            version: "9.9.9".into(),
            done: 0,
            total: None,
        };
        assert!(!updater.claim(downloading.clone()));
        updater.set(Status::UpToDate);
        assert!(updater.claim(downloading.clone()));
        assert!(!updater.claim(Status::Checking));
        assert_eq!(updater.status(), downloading);
    }

    #[test]
    fn compares_versions() {
        assert!(is_newer("0.5.0", "1.0.0"));
        assert!(is_newer("0.5.0", "0.5.1"));
        assert!(!is_newer("1.0.0", "1.0.0"));
        assert!(!is_newer("1.0.0", "0.9.9"));
    }

    #[test]
    fn has_an_asset_for_supported_platforms() {
        if cfg!(any(
            target_os = "macos",
            target_os = "linux",
            target_os = "windows"
        )) {
            assert!(asset_suffix().is_some());
        }
    }

    /// Talks to GitHub: `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn live_finds_releases() {
        // Pretend to be ancient so there's always something newer.
        let releases = ReleaseList::configure()
            .repo_owner(OWNER)
            .repo_name(REPO)
            .build()
            .unwrap()
            .fetch()
            .unwrap()
            .into_vec();
        assert!(!releases.is_empty());
        let suffix = asset_suffix().unwrap();
        assert!(
            releases
                .iter()
                .any(|r| r.assets().iter().any(|a| a.name().ends_with(suffix)))
        );
    }
}
