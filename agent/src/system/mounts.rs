//! Mounting: the kernel's own filesystems, scratch space and shares.

use std::ffi::CString;

/// `mount(2)` with no data.
pub fn mount(source: &str, target: &str, fstype: &str, flags: libc::c_ulong) -> Result<(), String> {
    std::fs::create_dir_all(target).map_err(|e| format!("{target}: {e}"))?;
    let c = |s: &str| CString::new(s).map_err(|e| e.to_string());
    let (source_c, target_c, fstype_c) = (c(source)?, c(target)?, c(fstype)?);
    // SAFETY: the strings are valid and NUL-terminated for the call.
    let result = unsafe {
        libc::mount(
            source_c.as_ptr(),
            target_c.as_ptr(),
            fstype_c.as_ptr(),
            flags,
            std::ptr::null(),
        )
    };
    if result == 0 {
        Ok(())
    } else {
        Err(format!(
            "mount {fstype} on {target}: {}",
            std::io::Error::last_os_error()
        ))
    }
}

/// proc, sys, dev and friends, plus writable scratch space: the root
/// filesystem itself is read-only.
pub fn kernel_filesystems() -> Result<(), String> {
    let nosuid = libc::MS_NOSUID | libc::MS_NODEV | libc::MS_NOEXEC;
    mount("proc", "/proc", "proc", nosuid)?;
    mount("sysfs", "/sys", "sysfs", nosuid)?;
    // Already there when the kernel mounts it itself.
    let _ = mount("devtmpfs", "/dev", "devtmpfs", libc::MS_NOSUID);
    mount(
        "devpts",
        "/dev/pts",
        "devpts",
        libc::MS_NOSUID | libc::MS_NOEXEC,
    )?;
    mount(
        "tmpfs",
        "/dev/shm",
        "tmpfs",
        libc::MS_NOSUID | libc::MS_NODEV,
    )?;
    mount("cgroup2", "/sys/fs/cgroup", "cgroup2", nosuid)?;
    for dir in ["/run", "/tmp", "/var/log", "/var/tmp"] {
        mount("tmpfs", dir, "tmpfs", libc::MS_NOSUID | libc::MS_NODEV)?;
    }
    Ok(())
}

/// The top folder of `path`: "/Users/me" → "/Users".
pub fn top(path: &str) -> Option<String> {
    let first = path.trim_start_matches('/').split('/').next()?;
    (!first.is_empty()).then(|| format!("/{first}"))
}

/// A folder the host shares over virtiofs, at `path` (the same path as
/// on the host, such as /Users/me).
pub fn share(tag: &str, path: &str) -> Result<(), String> {
    if std::fs::create_dir_all(path).is_err() {
        // The root is read-only: put scratch space on the top folder so
        // the mount point can be made.
        let top = top(path).ok_or_else(|| format!("can't share at {path}"))?;
        mount("tmpfs", &top, "tmpfs", libc::MS_NOSUID | libc::MS_NODEV)?;
    }
    mount(tag, path, "virtiofs", 0)
}

#[cfg(test)]
mod tests {
    #[test]
    fn finds_the_top_folder() {
        assert_eq!(super::top("/Users/me").as_deref(), Some("/Users"));
        assert_eq!(super::top("/home").as_deref(), Some("/home"));
        assert_eq!(super::top("/"), None);
    }
}
