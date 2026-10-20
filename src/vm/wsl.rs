//! Small pieces of working with WSL, kept apart from the Windows-only
//! code so they're tested everywhere.

use std::path::Path;

/// wsl.exe's output: UTF-16 by default, UTF-8 when it honours WSL_UTF8.
pub fn decode(bytes: &[u8]) -> String {
    let utf16 = bytes.len() >= 2 && bytes.iter().skip(1).step_by(2).all(|b| *b == 0);
    if utf16 {
        let units: Vec<u16> = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_le_bytes(*c))
            .collect();
        String::from_utf16_lossy(&units)
    } else {
        String::from_utf8_lossy(bytes).into_owned()
    }
}

/// Where a Windows path shows up inside WSL: `C:\Users\me` → `/mnt/c/Users/me`.
pub fn linux_path(path: &Path) -> Option<String> {
    let text = path.to_str()?;
    let mut chars = text.chars();
    let drive = chars.next().filter(char::is_ascii_alphabetic)?;
    let rest = chars.as_str().strip_prefix(':')?;
    Some(format!(
        "/mnt/{}{}",
        drive.to_ascii_lowercase(),
        rest.replace('\\', "/")
    ))
}

/// How WSL stands on this machine, for the engine's distro.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(windows), allow(dead_code))]
pub enum Wsl {
    /// Runs WSL 2 distros.
    Ready,
    /// Not installed, or not turned on: `wsl --install` sets it up.
    Missing,
    /// The old WSL that came with Windows: `wsl --update` brings the
    /// current one.
    Outdated,
}

/// How WSL stands, from whether `wsl --version` worked and what
/// `wsl --status` said (whether it worked, and its output). Only the
/// current WSL knows `--version`; the old one prints its help instead.
/// Messages are translated, so this goes by the parts that aren't: links,
/// error codes and commands.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn readiness(version: bool, status: (bool, &str)) -> Wsl {
    let text = status.1.to_lowercase();
    let said = |marks: &[&str]| marks.iter().any(|m| text.contains(m));
    // WSL 2 needs Windows' Virtual Machine Platform, which `--install`
    // turns on.
    let no_vm = said(&["hcs_e_hyperv_not_installed", "virtualmachineplatform"]);
    let not_installed = said(&[
        "aka.ms/wslinstall",
        "wsl_e_wsl_optional_component_required",
        "wsl.exe --install",
        "wsl --install",
    ]);
    if no_vm || (!version && not_installed) {
        Wsl::Missing
    } else if version || (status.0 && !said(&["--update"])) {
        Wsl::Ready
    } else {
        Wsl::Outdated
    }
}

/// Certificates (DER) as one PEM file.
pub fn pem<'a>(certificates: impl IntoIterator<Item = &'a [u8]>) -> String {
    let mut out = String::new();
    for der in certificates {
        out.push_str("-----BEGIN CERTIFICATE-----\n");
        let text = base64(der);
        for line in text.as_bytes().chunks(64) {
            out.push_str(&String::from_utf8_lossy(line));
            out.push('\n');
        }
        out.push_str("-----END CERTIFICATE-----\n");
    }
    out
}

fn base64(data: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let n = u32::from(chunk[0]) << 16
            | u32::from(*chunk.get(1).unwrap_or(&0)) << 8
            | u32::from(*chunk.get(2).unwrap_or(&0));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_wsl_output_either_way() {
        let utf16: Vec<u8> = "ferrodock\r\n"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect();
        assert_eq!(decode(&utf16), "ferrodock\r\n");
        assert_eq!(decode(b"ferrodock\n"), "ferrodock\n");
    }

    #[test]
    fn tells_how_wsl_stands() {
        // The current WSL, set up.
        assert_eq!(readiness(true, (true, "Default Version: 2")), Wsl::Ready);
        // Windows 11 with nothing installed yet.
        let none = "The Windows Subsystem for Linux is not installed. You can install by \
                    running 'wsl.exe --install'.\nFor more information please visit \
                    https://aka.ms/wslinstall";
        assert_eq!(readiness(false, (false, none)), Wsl::Missing);
        // Windows 10 with the feature turned off, in German.
        let off = "Die optionale Komponente \"Windows-Subsystem für Linux\" ist nicht \
                   aktiviert.\nWeitere Informationen finden Sie unter https://aka.ms/wslinstall";
        assert_eq!(readiness(false, (false, off)), Wsl::Missing);
        // The current WSL, without the Virtual Machine Platform.
        let no_vm = "Please enable the Virtual Machine Platform Windows feature.\n\
                     Error code: Wsl/Service/CreateInstance/CreateVm/HCS/HCS_E_HYPERV_NOT_INSTALLED";
        assert_eq!(readiness(true, (true, no_vm)), Wsl::Missing);
        // The old WSL: help for both, or a status asking for an update.
        let help = "Usage: wsl.exe [Argument] [Options...] [CommandLine]\n\
                    --update\n    Update the Windows Subsystem for Linux package.";
        assert_eq!(readiness(false, (false, help)), Wsl::Outdated);
        let kernel = "Default Version: 2\nWSL 2 requires an update to its kernel component. \
                      Run 'wsl.exe --update'.";
        assert_eq!(readiness(false, (true, kernel)), Wsl::Outdated);
        // The old WSL, working: no need to touch it.
        assert_eq!(
            readiness(false, (true, "Default Version: 2\nKernel version: 5.10.16")),
            Wsl::Ready
        );
    }

    #[test]
    fn finds_windows_paths_inside_wsl() {
        assert_eq!(
            linux_path(Path::new(r"C:\Users\me\AppData\Local\Ferrodock\certs.pem")).as_deref(),
            Some("/mnt/c/Users/me/AppData/Local/Ferrodock/certs.pem")
        );
        assert_eq!(linux_path(Path::new(r"\\server\share")), None);
    }

    #[test]
    fn writes_pem() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        let text = pem([&[0u8; 60][..]]);
        assert!(text.starts_with("-----BEGIN CERTIFICATE-----\nAAAA"));
        assert!(text.lines().all(|l| l.len() <= 64));
        assert!(text.ends_with("-----END CERTIFICATE-----\n"));
    }
}
