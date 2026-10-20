//! Which program runs Compose, in its own process: it changes HOME and
//! PATH.

use ferrodock_core::builtin::Paths;
use ferrodock_core::compose_cli::{Program, program};

#[test]
fn prefers_ferrodocks_own_compose_then_docker() {
    let home = std::env::temp_dir().join(format!("ferrodock-home-{}", std::process::id()));
    let tools = home.join("tools");
    std::fs::create_dir_all(&tools).unwrap();
    let set = |key: &str, value: &std::path::Path| {
        // SAFETY: this test is the only one in its process.
        unsafe { std::env::set_var(key, value) };
    };
    set(
        if cfg!(windows) {
            "LOCALAPPDATA"
        } else {
            "HOME"
        },
        &home,
    );
    set("PATH", &tools);

    // Neither: Compose has to be downloaded.
    assert_eq!(program(), None);

    // A `docker` command.
    let docker = tools.join(if cfg!(windows) {
        "docker.exe"
    } else {
        "docker"
    });
    std::fs::write(&docker, "").unwrap();
    assert_eq!(program(), Some(Program::Docker));

    // Ferrodock's own copy wins.
    let own = Paths::locate().unwrap().compose();
    std::fs::create_dir_all(own.parent().unwrap()).unwrap();
    std::fs::write(&own, "").unwrap();
    assert_eq!(program(), Some(Program::Standalone(own)));

    std::fs::remove_dir_all(&home).unwrap();
}
