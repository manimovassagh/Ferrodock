//! Embeds the app icon and version info into the Windows executable, and
//! tells the code whether this platform has Ferrodock's own engine.

fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    // `cfg(engine)`: where Ferrodock runs its own Docker engine: in a VM
    // on Apple Silicon Macs, in a WSL distro on Windows (x86_64), and as a
    // system service on Linux (x86_64 and arm64).
    println!("cargo::rustc-check-cfg=cfg(engine)");
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    if matches!(
        (os.as_str(), arch.as_str()),
        ("macos", "aarch64") | ("windows", "x86_64") | ("linux", "x86_64" | "aarch64")
    ) {
        println!("cargo::rustc-cfg=engine");
    }
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.compile().expect("failed to embed Windows resources");
    }
}
