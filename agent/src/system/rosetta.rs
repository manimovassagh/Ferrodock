//! Rosetta, on Apple Silicon: the host shares Apple's x86_64 translator
//! over virtiofs, and the kernel runs every x86_64 (amd64) program
//! through it, so amd64-only images work too.

use super::mounts::mount;

/// Where the share is mounted: /run is writable, the root isn't.
const MOUNT: &str = "/run/rosetta";
const BINFMT: &str = "/proc/sys/fs/binfmt_misc";
/// The start of an x86_64 ELF program, and which of its bits count.
const MAGIC: &str = r"\x7fELF\x02\x01\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00\x02\x00\x3e\x00";
const MASK: &str =
    r"\xff\xff\xff\xff\xff\xfe\xfe\x00\xff\xff\xff\xff\xff\xff\xff\xff\xfe\xff\xff\xff";

/// The binfmt_misc rule. C: a program runs with its own file's
/// credentials (setuid works); F: the kernel opens the translator now,
/// so it's found from inside containers too.
pub fn rule(interpreter: &str) -> String {
    format!(":rosetta:M::{MAGIC}:{MASK}:{interpreter}:CF")
}

/// Mounts the share with tag `tag` and hands x86_64 programs to it.
pub fn enable(tag: &str) -> Result<(), String> {
    mount(tag, MOUNT, "virtiofs", 0)?;
    let register = format!("{BINFMT}/register");
    if !std::path::Path::new(&register).exists() {
        let nosuid = libc::MS_NOSUID | libc::MS_NODEV | libc::MS_NOEXEC;
        mount("binfmt_misc", BINFMT, "binfmt_misc", nosuid)?;
    }
    std::fs::write(&register, rule(&format!("{MOUNT}/rosetta")))
        .map_err(|e| format!("{register}: {e}"))
}

#[cfg(test)]
mod tests {
    /// `\xNN` escapes, read as the kernel reads them.
    fn bytes(escaped: &str) -> Vec<u8> {
        let mut out = Vec::new();
        let mut rest = escaped;
        while !rest.is_empty() {
            if let Some(hex) = rest.strip_prefix(r"\x") {
                out.push(u8::from_str_radix(&hex[..2], 16).unwrap());
                rest = &hex[2..];
            } else {
                out.push(rest.as_bytes()[0]);
                rest = &rest[1..];
            }
        }
        out
    }

    #[test]
    fn matches_x86_64_programs_only() {
        let (magic, mask) = (bytes(super::MAGIC), bytes(super::MASK));
        assert_eq!(magic.len(), 20);
        assert_eq!(mask.len(), 20);
        let matches = |header: &[u8]| (0..20).all(|i| header[i] & mask[i] == magic[i] & mask[i]);
        assert!(matches(&magic));
        // Shared objects (type 3) too, not only executables (type 2).
        let mut library = magic.clone();
        library[16] = 3;
        assert!(matches(&library));
        // An arm64 program (machine 0xb7) runs as it is.
        let mut arm64 = magic.clone();
        arm64[18] = 0xb7;
        assert!(!matches(&arm64));
    }

    #[test]
    fn registers_the_translator() {
        let rule = super::rule("/run/rosetta/rosetta");
        assert!(rule.starts_with(r":rosetta:M::\x7fELF"));
        assert!(rule.ends_with(":/run/rosetta/rosetta:CF"));
        // name, type, offset, magic, mask, interpreter, flags.
        assert_eq!(rule.split(':').count(), 8);
    }
}
