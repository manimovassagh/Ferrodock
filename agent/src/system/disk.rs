//! The data disk: formatted on first boot, then where the engine keeps
//! images, containers and volumes.

use std::io::{Read, Seek, SeekFrom};

use super::mounts::mount;
use crate::log;

/// Where the data disk is mounted; the engine's folders point into it.
const DATA: &str = "/data";

/// Whether `bytes` (the start of a disk) hold an ext2/3/4 filesystem: its
/// superblock starts at 1024, with the magic number 0xEF53 at offset 56.
pub fn has_ext4(bytes: &[u8]) -> bool {
    bytes.len() >= 1082 && bytes[1080] == 0x53 && bytes[1081] == 0xEF
}

fn formatted(device: &str) -> Result<bool, String> {
    let mut disk = std::fs::File::open(device).map_err(|e| format!("{device}: {e}"))?;
    let mut start = vec![0u8; 2048];
    disk.seek(SeekFrom::Start(0))
        .and_then(|_| disk.read_exact(&mut start))
        .map_err(|e| format!("{device}: {e}"))?;
    Ok(has_ext4(&start))
}

/// Formats `device` if it's blank, mounts it, grows its filesystem to
/// fill it, and points the engine's
/// state folders at it.
pub fn mount_data(device: &str) -> Result<(), String> {
    if !formatted(device)? {
        log(&format!("formatting {device}"));
        super::run("mkfs.ext4", &["-q", "-F", "-L", "ferrodock-data", device])?;
    }
    mount(device, DATA, "ext4", libc::MS_NOATIME)?;
    // The disk grows when a bigger size is picked in Ferrodock: grow the
    // filesystem to match (online; nothing to do when it fits already).
    if let Err(e) = super::run("resize2fs", &[device]) {
        log(&format!("growing {device}: {e}"));
    }
    for name in ["docker", "containerd"] {
        let stored = format!("{DATA}/{name}");
        let used = format!("/var/lib/{name}");
        std::fs::create_dir_all(&stored).map_err(|e| format!("{stored}: {e}"))?;
        mount(&stored, &used, "", libc::MS_BIND)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_a_formatted_disk() {
        let mut disk = vec![0u8; 2048];
        assert!(!has_ext4(&disk));
        disk[1080] = 0x53;
        disk[1081] = 0xEF;
        assert!(has_ext4(&disk));
        assert!(!has_ext4(&disk[..100]));
    }
}
