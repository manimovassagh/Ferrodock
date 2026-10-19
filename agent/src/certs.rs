//! Certificates the host trusts, added to the engine's own. Behind a
//! company proxy that inspects TLS, pulling images then works just as it
//! does on the host.

use std::path::Path;

/// The bundle the engine (and everything else in the image) trusts.
const BUNDLE: &str = "/etc/ssl/certs/ca-certificates.crt";

/// Adds the PEM certificates in `extra` to the engine's bundle.
pub fn add(extra: &Path) -> Result<(), String> {
    add_to(Path::new(BUNDLE), extra)
}

/// The bundle becomes the image's original one plus `extra`, so adding
/// again on the next start doesn't pile them up.
fn add_to(bundle: &Path, extra: &Path) -> Result<(), String> {
    let original = bundle.with_extension("crt.orig");
    if !original.exists() {
        std::fs::copy(bundle, &original).map_err(|e| format!("{}: {e}", bundle.display()))?;
    }
    let mut text = std::fs::read_to_string(&original).map_err(|e| e.to_string())?;
    let extra = std::fs::read_to_string(extra).map_err(|e| format!("{}: {e}", extra.display()))?;
    if !text.ends_with('\n') {
        text.push('\n');
    }
    text.push_str(&extra);
    std::fs::write(bundle, text).map_err(|e| format!("{}: {e}", bundle.display()))
}

#[cfg(test)]
mod tests {
    #[test]
    fn adds_once_however_often_it_starts() {
        let dir = std::env::temp_dir().join(format!("ferrodock-certs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let bundle = dir.join("ca-certificates.crt");
        let extra = dir.join("host.pem");
        std::fs::write(&bundle, "IMAGE").unwrap();
        std::fs::write(&extra, "HOST\n").unwrap();
        super::add_to(&bundle, &extra).unwrap();
        super::add_to(&bundle, &extra).unwrap();
        assert_eq!(std::fs::read_to_string(&bundle).unwrap(), "IMAGE\nHOST\n");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
