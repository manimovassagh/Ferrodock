//! The signature of a release's `SHA256SUMS.txt`.
//!
//! The release workflow signs the checksums with a minisign key kept in the
//! repository's secrets; its public key is built into the app from
//! `packaging/minisign.pub`. Someone able to replace a release's files then
//! still can't make the app install them: the checksums wouldn't be signed.
//! While that file is empty, updates are checked against the checksums
//! only, as before.

use minisign_verify::{PublicKey, Signature};

/// The signature's file name in a release.
pub const SIG_NAME: &str = "SHA256SUMS.txt.minisig";

const PUBLIC_KEY: &str = include_str!("../../packaging/minisign.pub");

/// The public key releases are signed with, when this build has one.
pub fn public_key() -> Option<&'static str> {
    key_line(PUBLIC_KEY)
}

/// The key itself, without minisign's comment line.
fn key_line(file: &str) -> Option<&str> {
    file.lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with("untrusted comment:"))
}

/// Checks that `sig` (a `.minisig` file) signs `data` with `key`.
pub fn verify(key: &str, data: &[u8], sig: &str) -> Result<(), String> {
    let key = PublicKey::from_base64(key).map_err(|e| format!("bad public key: {e}"))?;
    let sig = Signature::decode(sig).map_err(|_| "the release's signature is damaged")?;
    key.verify(data, &sig, false)
        .map_err(|_| "the release's checksums aren't signed by Ferrodock's key".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = include_str!("testdata/test.pub");
    const SUMS: &[u8] = include_bytes!("testdata/sums.txt");
    const SIG: &str = include_str!("testdata/sums.txt.minisig");

    #[test]
    fn reads_the_key_line() {
        assert_eq!(key_line(""), None);
        assert_eq!(key_line("\n"), None);
        assert_eq!(
            key_line(KEY),
            Some("RWSCogz7iDxvRGJazqMCEkL4j0yCbC3dVwJajnYen8YL8CNaX8L3GB7e")
        );
    }

    #[test]
    fn accepts_signed_checksums() {
        verify(key_line(KEY).unwrap(), SUMS, SIG).unwrap();
    }

    #[test]
    fn refuses_changed_checksums() {
        let changed = b"0000  Ferrodock-9.9.9-linux-x64.tar.gz\n";
        let error = verify(key_line(KEY).unwrap(), changed, SIG).unwrap_err();
        assert!(error.contains("aren't signed"), "{error}");
    }

    #[test]
    fn refuses_a_damaged_signature() {
        assert!(verify(key_line(KEY).unwrap(), SUMS, "not a signature").is_err());
    }

    /// No key in this build yet: updates rely on the checksums alone.
    #[test]
    fn has_no_key_until_one_is_added() {
        assert_eq!(public_key(), key_line(PUBLIC_KEY));
    }
}
