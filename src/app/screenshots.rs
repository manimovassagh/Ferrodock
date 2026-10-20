//! Renders the app offscreen against the local Docker Engine and saves
//! PNGs, for checking the design and refreshing the README screenshots:
//!
//! ```sh
//! SCREENSHOTS=docs/screenshots cargo test screenshots -- --ignored
//! ```

use std::time::{Duration, Instant};

use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;

use super::FerrodockApp;
use super::settings::Theme;

const SIZE: egui::Vec2 = egui::vec2(1240.0, 740.0);

fn harness() -> Harness<'static, FerrodockApp> {
    harness_of_size(SIZE)
}

fn harness_of_size(size: egui::Vec2) -> Harness<'static, FerrodockApp> {
    Harness::builder()
        .with_size(size)
        .with_pixels_per_point(2.0)
        .wgpu()
        .build_eframe(|cc| FerrodockApp::new(cc))
}

/// Runs frames until the worker has had time to load everything.
fn settle(h: &mut Harness<'static, FerrodockApp>, time: Duration) {
    let start = Instant::now();
    while start.elapsed() < time {
        h.step();
        std::thread::sleep(Duration::from_millis(50));
    }
    h.run_steps(4);
}

fn save(h: &mut Harness<'static, FerrodockApp>, name: &str) {
    // Keep the pointer (and hover effects) out of the picture.
    h.event(egui::Event::PointerGone);
    h.run_steps(3);
    let dir = std::env::var("SCREENSHOTS").unwrap_or_else(|_| "target/screenshots".into());
    std::fs::create_dir_all(&dir).unwrap();
    let image = h.render().expect("offscreen rendering works");
    let path = format!("{dir}/{name}.png");
    image.save(&path).unwrap();
    println!("saved {path}");
}

fn set_theme(h: &mut Harness<'static, FerrodockApp>, theme: Theme) {
    h.state_mut().settings.theme = theme;
    let ctx = h.ctx.clone();
    h.state().settings.apply(&ctx);
    h.run_steps(3);
}

/// Opens a page and selects its first item that `pick` likes.
fn open(h: &mut Harness<'static, FerrodockApp>, page: usize, select: Option<String>) {
    h.state_mut().open_page(page);
    if let Some(id) = select {
        h.state_mut().pages[page].select(&id);
    }
    settle(h, Duration::from_millis(1500));
}

fn click(h: &mut Harness<'static, FerrodockApp>, label: &str) {
    h.get_by_label(label).click();
    h.run_steps(2);
}

/// The first running container with a published port, for the details shots.
fn busiest(h: &Harness<'static, FerrodockApp>) -> Option<String> {
    let containers = &h.state().store.containers;
    containers
        .iter()
        .find(|c| c.is_running() && !c.ports.is_empty())
        .or_else(|| containers.iter().find(|c| c.is_running()))
        .map(|c| c.id.clone())
}

/// Every screenshot used in the README and on the website.
#[test]
#[ignore]
fn screenshots() {
    let mut h = harness();
    settle(&mut h, Duration::from_secs(3));
    set_theme(&mut h, Theme::Light);

    open(&mut h, 0, None);
    // Let a few CPU readings come in for the sparklines.
    settle(&mut h, Duration::from_secs(10));
    save(&mut h, "containers");

    let busy = busiest(&h);
    // A container that writes a steady stream of logs, if there is one.
    let chatty = h
        .state()
        .store
        .containers
        .iter()
        .find(|c| c.name == "payments")
        .or_else(|| {
            h.state()
                .store
                .containers
                .iter()
                .find(|c| c.name == "api-server")
        })
        .map(|c| c.id.clone())
        .or(busy.clone());
    open(&mut h, 0, chatty);
    click(&mut h, "Logs");
    click(&mut h, "Time");
    settle(&mut h, Duration::from_secs(2));
    save(&mut h, "logs");

    click(&mut h, "Stats");
    settle(&mut h, Duration::from_secs(6));
    save(&mut h, "stats");

    click(&mut h, "Top");
    settle(&mut h, Duration::from_secs(3));
    save(&mut h, "processes");

    let volume = h.state().store.volumes.first().map(|v| v.name.clone());
    open(&mut h, 3, volume);
    save(&mut h, "volumes");
    h.state_mut().pages[3].clear_selection();

    let network = h
        .state()
        .store
        .networks
        .iter()
        .find(|n| !n.is_builtin())
        .map(|n| n.id.clone());
    open(&mut h, 4, network);
    save(&mut h, "network");
    click(&mut h, "New network");
    settle(&mut h, Duration::from_millis(500));
    save(&mut h, "new-network");
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    h.state_mut().pages[4].clear_selection();

    open(&mut h, 5, None);
    save(&mut h, "disk");

    let image = h.state().store.images.first().map(|i| i.id.clone());
    open(&mut h, 2, image);
    click(&mut h, "Run…");
    settle(&mut h, Duration::from_secs(1));
    save(&mut h, "run");
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    click(&mut h, "HISTORY");
    settle(&mut h, Duration::from_secs(1));
    save(&mut h, "history");
    h.state_mut().pages[2].clear_selection();

    h.state().updater.pretend(crate::update::Status::Available(
        crate::update::Available::example("1.0.0"),
    ));
    open(&mut h, 0, None);
    save(&mut h, "update-available");
    h.state().updater.pretend(crate::update::Status::Idle);

    set_theme(&mut h, Theme::Dark);
    open(&mut h, 0, busy);
    click(&mut h, "Overview");
    save(&mut h, "dark-mode");
    click(&mut h, "More actions");
    click(&mut h, "Duplicate…");
    settle(&mut h, Duration::from_millis(500));
    save(&mut h, "duplicate");
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    click(&mut h, "More actions");
    click(&mut h, "Edit…");
    settle(&mut h, Duration::from_millis(500));
    save(&mut h, "edit");
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    click(&mut h, "Change…");
    settle(&mut h, Duration::from_millis(500));
    save(&mut h, "limits");
}

/// The states new users see: nothing found, and no Docker at all.
#[test]
#[ignore]
fn screenshots_of_empty_states() {
    let mut h = harness();
    settle(&mut h, Duration::from_secs(3));
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    settle(&mut h, Duration::from_millis(500));
    save(&mut h, "bulk");
    h.key_press(egui::Key::Escape);
    h.run_steps(3);

    h.state_mut().search = "no-such-thing".into();
    settle(&mut h, Duration::from_millis(500));
    save(&mut h, "empty-search");

    // SAFETY: tests that use DOCKER_HOST run one at a time (`--ignored`,
    // single test); nothing else reads it concurrently here.
    unsafe { std::env::set_var("DOCKER_HOST", "unix:///nonexistent/docker.sock") };
    let mut h = harness();
    settle(&mut h, Duration::from_secs(3));
    save(&mut h, "no-docker");
    unsafe { std::env::remove_var("DOCKER_HOST") };
}

/// The Files tab of the first container, a folder and a previewed file.
/// Works with stopped containers too (their files come from the archive
/// API).
#[test]
#[ignore]
fn screenshots_of_files() {
    let mut h = harness();
    settle(&mut h, Duration::from_secs(3));
    set_theme(&mut h, Theme::Light);
    let first = h.state().store.containers.first().map(|c| c.id.clone());
    open(&mut h, 0, first);
    click(&mut h, "Files");
    settle(&mut h, Duration::from_secs(2));
    save(&mut h, "files");
    click(&mut h, "etc");
    settle(&mut h, Duration::from_secs(2));
    click(&mut h, "hosts");
    settle(&mut h, Duration::from_secs(2));
    save(&mut h, "files-preview");
    // A link to a file opens as a preview, not an empty folder.
    click(&mut h, "mtab");
    settle(&mut h, Duration::from_secs(3));
    save(&mut h, "files-link");

    // A volume, read-only through a helper container.
    let volume = h.state().store.volumes.first().map(|v| v.name.clone());
    open(&mut h, 3, volume);
    click(&mut h, "Browse files");
    settle(&mut h, Duration::from_secs(3));
    save(&mut h, "volume-files");
    click(&mut h, "Done browsing");
    settle(&mut h, Duration::from_secs(1));
}

/// The Terminal tab with a live shell, in a throwaway container.
#[test]
#[ignore]
fn screenshots_of_terminal() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let client = crate::docker::DockerClient::connect().unwrap();
    let image = runtime
        .block_on(client.list_images())
        .unwrap()
        .into_iter()
        .next()
        .expect("an image to run");
    let spec = crate::docker::RunSpec {
        image: image.id,
        name: "ferrodock-shell-demo".into(),
        command: "sleep 600".into(),
        ..Default::default()
    };
    let _ = runtime.block_on(client.perform(
        crate::docker::Action::Remove(crate::docker::Resource::Containers),
        &spec.name,
    ));
    let id = runtime.block_on(client.run(&spec)).unwrap();

    let mut h = harness();
    settle(&mut h, Duration::from_secs(3));
    set_theme(&mut h, Theme::Light);
    open(&mut h, 0, Some(id.clone()));
    click(&mut h, "Terminal");
    settle(&mut h, Duration::from_secs(2));
    for line in ["ls -l /", "echo \"Hello from $(hostname)\""] {
        h.event(egui::Event::Text(line.into()));
        h.key_press(egui::Key::Enter);
        settle(&mut h, Duration::from_secs(1));
    }
    save(&mut h, "terminal");
    h.event(egui::Event::Text("top".into()));
    h.key_press(egui::Key::Enter);
    settle(&mut h, Duration::from_secs(2));
    set_theme(&mut h, Theme::Dark);
    save(&mut h, "terminal-top");

    let _ = runtime.block_on(client.perform(crate::docker::Action::Stop, &id));
    let _ = runtime.block_on(client.perform(
        crate::docker::Action::Remove(crate::docker::Resource::Containers),
        &id,
    ));
}

/// A container without the asked-for shell, then the debug shell next to
/// it. Set `FERRODOCK_DEBUG_IMAGE` to a local image if Docker Hub can't be
/// reached.
#[test]
#[ignore]
fn screenshots_of_debug_shell() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let client = crate::docker::DockerClient::connect().unwrap();
    let image = runtime
        .block_on(client.list_images())
        .unwrap()
        .into_iter()
        .next()
        .expect("an image to run");
    let spec = crate::docker::RunSpec {
        image: image.id,
        name: "ferrodock-debug-demo".into(),
        command: "sleep 600".into(),
        ..Default::default()
    };
    let remove = |id: &str| {
        let _ = runtime.block_on(client.perform(crate::docker::Action::Stop, id));
        let _ = runtime.block_on(client.perform(
            crate::docker::Action::Remove(crate::docker::Resource::Containers),
            id,
        ));
    };
    remove(&spec.name);
    let id = runtime.block_on(client.run(&spec)).unwrap();

    let mut h = harness();
    settle(&mut h, Duration::from_secs(3));
    set_theme(&mut h, Theme::Light);
    open(&mut h, 0, Some(id.clone()));
    click(&mut h, "Terminal");
    settle(&mut h, Duration::from_secs(1));
    // Ask for a shell the container doesn't have.
    h.get_by_label("Shell").click();
    h.run_steps(2);
    h.event(egui::Event::Text("/bin/no-such-shell".into()));
    click(&mut h, "New session");
    settle(&mut h, Duration::from_secs(3));
    save(&mut h, "no-shell");
    click(&mut h, "Open a debug shell");
    settle(&mut h, Duration::from_secs(4));
    for line in ["ps", "ls /target/"] {
        h.event(egui::Event::Text(line.into()));
        h.key_press(egui::Key::Enter);
        settle(&mut h, Duration::from_secs(1));
    }
    save(&mut h, "debug-shell");
    remove(&id);
}

/// Docker Hub search, and the Tag dialog, on the Images page.
#[test]
#[ignore]
fn screenshots_of_image_tools() {
    let mut h = harness();
    settle(&mut h, Duration::from_secs(3));
    set_theme(&mut h, Theme::Light);
    let first = h.state().store.images.first().map(|i| i.id.clone());
    open(&mut h, 2, first);
    h.get_by_role_and_label(egui::accesskit::Role::TextInput, "Pull")
        .click();
    h.run_steps(2);
    h.event(egui::Event::Text("nginx".into()));
    // Results come by themselves once typing pauses.
    settle(&mut h, Duration::from_secs(5));
    save(&mut h, "image-search");
    click(&mut h, "More actions");
    click(&mut h, "Tag…");
    settle(&mut h, Duration::from_millis(500));
    h.event(egui::Event::Text("ghcr.io/me/app:1.0".into()));
    h.run_steps(3);
    save(&mut h, "image-tag");
}

/// The engine picker, and switching to another engine without restarting.
/// A second Docker context ("staging") reaches the same daemon through
/// another socket path, so the switch is real but the data stays the same.
#[test]
#[ignore]
fn screenshots_of_engines() {
    let config = std::env::temp_dir().join(format!("ferrodock-config-{}", std::process::id()));
    let meta = config.join("contexts/meta/0123abcd");
    std::fs::create_dir_all(&meta).unwrap();
    let socket = config.join("staging.sock");
    let _ = std::fs::remove_file(&socket);
    #[cfg(unix)]
    std::os::unix::fs::symlink("/var/run/docker.sock", &socket).unwrap();
    std::fs::write(
        meta.join("meta.json"),
        format!(
            r#"{{"Name":"staging","Endpoints":{{"docker":{{"Host":"unix://{}"}}}}}}"#,
            socket.display()
        ),
    )
    .unwrap();
    // Only this test runs in its process when it's run on its own.
    unsafe { std::env::set_var("DOCKER_CONFIG", &config) };

    let mut h = harness();
    settle(&mut h, Duration::from_secs(3));
    set_theme(&mut h, Theme::Light);
    h.state_mut().settings.engine = None;
    click(&mut h, "Docker engine");
    settle(&mut h, Duration::from_millis(500));
    save(&mut h, "engines");
    click(&mut h, "staging");
    settle(&mut h, Duration::from_secs(4));
    save(&mut h, "engine-switched");
    let state = h.state();
    assert_eq!(state.store.engine.as_deref(), Some("staging"));
    assert!(
        !state.store.containers.is_empty(),
        "reloaded from the new engine"
    );
    assert!(matches!(
        state.store.connection,
        crate::app::store::Connection::Connected { .. }
    ));
    std::fs::remove_dir_all(&config).unwrap();
}

/// Brings a two-service project up from its compose.yaml through the UI,
/// then shows its merged logs. Takes it down again at the end.
#[test]
#[ignore]
fn screenshots_of_compose_up() {
    use egui::accesskit::Role;
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let client = crate::docker::DockerClient::connect().unwrap();
    let image = runtime
        .block_on(client.list_images())
        .unwrap()
        .into_iter()
        .next()
        .expect("an image to run");
    let tag = image.tags.first().cloned().unwrap_or(image.id);
    let dir = std::env::temp_dir().join("shop");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("compose.yaml");
    std::fs::write(
        &file,
        format!(
            "services:\n\
             \x20 web:\n    image: {tag}\n    command: sh -c 'while true; do echo GET / 200; sleep 1; done'\n\
             \x20 worker:\n    image: {tag}\n    command: sh -c 'while true; do echo processed job; sleep 2; done'\n"
        ),
    )
    .unwrap();

    let mut h = harness();
    settle(&mut h, Duration::from_secs(3));
    set_theme(&mut h, Theme::Light);
    open(&mut h, 1, None);
    h.get_by_role_and_label(Role::TextInput, "Up").click();
    h.run_steps(2);
    h.event(egui::Event::Text(file.display().to_string()));
    h.run_steps(2);
    h.get_by_role_and_label(Role::Button, "Up").click();
    settle(&mut h, Duration::from_secs(8));
    save(&mut h, "compose-up");
    let up_ok = h
        .state()
        .store
        .compose
        .run
        .as_ref()
        .is_some_and(|j| j.ended == Some(None));
    h.state_mut().pages[1].select("shop");
    settle(&mut h, Duration::from_secs(1));
    click(&mut h, "Logs");
    settle(&mut h, Duration::from_secs(5));
    save(&mut h, "compose-logs");

    let down = ["-p", "shop", "down", "-t", "1"].map(str::to_owned);
    let _ = runtime.block_on(crate::docker::compose_cli::stream(
        crate::docker::compose_cli::command(None, None, &down),
        |_| {},
    ));
    std::fs::remove_dir_all(&dir).unwrap();
    assert!(up_ok, "compose up finished without errors");
}

/// The container list grouped by Compose project.
#[test]
#[ignore]
fn screenshots_of_grouping() {
    let mut h = harness();
    settle(&mut h, Duration::from_secs(3));
    set_theme(&mut h, Theme::Light);
    open(&mut h, 0, None);
    save(&mut h, "before-grouping");
    click(&mut h, "Group by project");
    settle(&mut h, Duration::from_secs(1));
    save(&mut h, "grouped");
    click(&mut h, "Group by project");
    h.run_steps(2);
}

/// Ticked containers with the bulk bar, the removal question, and the
/// Reset Docker question (never confirmed).
#[test]
#[ignore]
fn screenshots_of_bulk_and_reset() {
    let mut h = harness();
    settle(&mut h, Duration::from_secs(3));
    set_theme(&mut h, Theme::Light);
    open(&mut h, 0, None);
    let names: Vec<String> = h
        .state()
        .store
        .containers
        .iter()
        .take(3)
        .map(|c| c.name.clone())
        .collect();
    for name in &names {
        click(&mut h, &format!("Select {name}"));
    }
    save(&mut h, "bulk-selected");
    click(&mut h, &format!("Remove {}…", names.len()));
    save(&mut h, "bulk-remove");
    click(&mut h, "Cancel");
    click(&mut h, "Select all");
    save(&mut h, "bulk-all");
    click(&mut h, "Select none");

    open(&mut h, 5, None);
    save(&mut h, "disk-danger-zone");
    click(&mut h, "Reset Docker…");
    h.get_by_label("Type “reset” to confirm").type_text("res");
    h.run_steps(3);
    save(&mut h, "reset");
    click(&mut h, "Cancel");
    assert!(h.state().confirmation.is_none());
}

/// With the engine down: Ferrodock starts it at launch and connects.
/// Needs a stopped engine that `systemctl start docker` (or the platform's
/// starter) brings up.
#[test]
#[ignore]
fn screenshots_of_engine_start() {
    let mut h = harness();
    set_theme(&mut h, Theme::Light);
    settle(&mut h, Duration::from_millis(1500));
    save(&mut h, "engine-starting");
    let start = Instant::now();
    while !matches!(
        h.state().store.connection,
        super::Connection::Connected { .. }
    ) && start.elapsed() < Duration::from_secs(60)
    {
        settle(&mut h, Duration::from_millis(500));
    }
    settle(&mut h, Duration::from_secs(2));
    save(&mut h, "engine-started");
    assert!(matches!(
        h.state().store.connection,
        super::Connection::Connected { .. }
    ));
}

/// The Quick start page, a template's Run dialog, a real run of Redis
/// (pulled first; the container is left for the caller to remove), and
/// Run on Docker Hub results.
#[test]
#[ignore]
fn screenshots_of_quick_start() {
    let mut h = harness();
    settle(&mut h, Duration::from_secs(3));
    set_theme(&mut h, Theme::Light);
    let page = h
        .state()
        .pages
        .iter()
        .position(|p| p.title() == "Quick start")
        .unwrap();
    open(&mut h, page, None);
    save(&mut h, "quick-start");

    h.state_mut().pages[page].select("Postgres");
    h.run_steps(3);
    save(&mut h, "quick-start-run");
    click(&mut h, "Cancel");

    h.state_mut().pages[page].select("Redis");
    h.run_steps(3);
    click(&mut h, "Run");
    let start = Instant::now();
    while !h
        .state()
        .store
        .containers
        .iter()
        .any(|c| c.name == "redis" && c.is_running())
        && start.elapsed() < Duration::from_secs(120)
    {
        settle(&mut h, Duration::from_millis(500));
    }
    settle(&mut h, Duration::from_secs(1));
    save(&mut h, "quick-start-running");
    assert!(h.state().store.containers.iter().any(|c| c.name == "redis"));

    open(&mut h, 2, None);
    h.get_by_role_and_label(egui::accesskit::Role::TextInput, "Pull")
        .click();
    h.run_steps(2);
    h.event(egui::Event::Text("postgres".into()));
    settle(&mut h, Duration::from_secs(5));
    save(&mut h, "hub-run");
}

#[test]
#[ignore]
fn screenshots_of_integrations() {
    let mut h = harness();
    settle(&mut h, Duration::from_secs(2));
    for (theme, name) in [(Theme::Light, "light"), (Theme::Dark, "dark")] {
        set_theme(&mut h, theme);
        h.state_mut().show_integrations = false;
        h.run_steps(3);
        save(&mut h, &format!("integrations-sidebar-{name}"));
        h.state_mut().show_integrations = true;
        h.run_steps(4);
        save(&mut h, &format!("integrations-dialog-{name}"));
        click(&mut h, "Cursor");
        h.run_steps(3);
        save(&mut h, &format!("integrations-cursor-{name}"));
        click(&mut h, "Claude Code");
    }
}

#[test]
#[ignore]
fn screenshots_of_themes() {
    let mut h = harness();
    settle(&mut h, Duration::from_secs(2));
    open(&mut h, 2, None);
    for editor in &crate::theme::EDITOR_THEMES {
        h.state_mut().settings.editor_theme = Some(editor.id.to_owned());
        let ctx = h.ctx.clone();
        h.state().settings.apply(&ctx);
        h.run_steps(3);
        save(&mut h, &format!("theme-{}", editor.id));
    }
    h.state_mut().show_settings = true;
    h.run_steps(4);
    save(&mut h, "theme-settings");
}

/// Settings → Ferrodock's engine with the engine running, and the
/// question before resetting it. Nothing is clicked that changes it.
#[test]
#[ignore]
fn screenshots_of_engine_settings() {
    let mut h = harness();
    settle(&mut h, Duration::from_secs(2));
    set_theme(&mut h, Theme::Light);
    h.state()
        .engine_starter
        .builtin
        .pretend(crate::vm::State::Running);
    h.state_mut().show_settings = true;
    h.run_steps(4);
    save(&mut h, "engine-settings");
    click(&mut h, "Reset engine…");
    h.run_steps(4);
    save(&mut h, "engine-reset");
}

/// Scrolls what's under the middle of the window all the way down.
fn scroll_to_bottom(h: &mut Harness<'static, FerrodockApp>) {
    let center = h.ctx.content_rect().center();
    h.event(egui::Event::PointerMoved(center));
    h.event(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::vec2(0.0, -5000.0),
        phase: egui::TouchPhase::Move,
        source: egui::MouseWheelSource::Unknown,
        modifiers: egui::Modifiers::NONE,
    });
    h.run_steps(10);
}

/// The dialogs in the smallest window, at the largest interface size:
/// they fit in it, and their bodies scroll.
#[test]
#[ignore]
fn screenshots_of_dialogs_in_a_small_window() {
    let mut h = harness_of_size(egui::vec2(820.0, 480.0));
    settle(&mut h, Duration::from_secs(2));
    h.state_mut().settings.scale = super::settings::Settings::MAX_SCALE;
    set_theme(&mut h, Theme::Light);
    h.state()
        .engine_starter
        .builtin
        .pretend(crate::vm::State::Running);
    h.state_mut().show_settings = true;
    h.run_steps(4);
    save(&mut h, "small-settings-top");
    scroll_to_bottom(&mut h);
    save(&mut h, "small-settings-bottom");
    h.get_by_label("Reset engine…").scroll_to_me();
    h.run_steps(4);
    save(&mut h, "small-settings-engine");
    click(&mut h, "Reset engine…");
    h.run_steps(4);
    save(&mut h, "small-engine-reset");
    click(&mut h, "Cancel");
    h.state_mut().show_settings = false;

    h.state_mut().show_integrations = true;
    h.run_steps(4);
    save(&mut h, "small-integrations-top");
    scroll_to_bottom(&mut h);
    save(&mut h, "small-integrations-bottom");
    h.state_mut().show_integrations = false;

    h.state_mut().show_help = true;
    h.run_steps(4);
    save(&mut h, "small-shortcuts");
}

/// Numbered frames for the README demo animation, with how long each one
/// shows, listed in `$SCREENSHOTS/demo/frames.txt` as `file milliseconds`.
struct Recorder {
    dir: String,
    frames: Vec<(String, u32)>,
}

impl Recorder {
    fn new() -> Self {
        let dir = std::env::var("SCREENSHOTS").unwrap_or_else(|_| "target/screenshots".into());
        let dir = format!("{dir}/demo");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self {
            dir,
            frames: Vec::new(),
        }
    }

    /// Renders the window as it is now, shown for `ms`.
    fn frame(&mut self, h: &mut Harness<'static, FerrodockApp>, ms: u32) {
        let name = format!("{:03}.png", self.frames.len());
        let image = h.render().expect("offscreen rendering works");
        image.save(format!("{}/{name}", self.dir)).unwrap();
        self.frames.push((name, ms));
    }

    /// A frame every `every` ms of real time for `total` ms, so logs and
    /// graphs move.
    fn live(&mut self, h: &mut Harness<'static, FerrodockApp>, total: u32, every: u32) {
        for _ in 0..total / every {
            settle(h, Duration::from_millis(every.into()));
            self.frame(h, every);
        }
    }

    fn finish(self) {
        let list: String = self
            .frames
            .iter()
            .map(|(name, ms)| format!("{name} {ms}\n"))
            .collect();
        std::fs::write(format!("{}/frames.txt", self.dir), list).unwrap();
        println!("saved {} frames in {}", self.frames.len(), self.dir);
    }
}

/// Moves the pointer over a widget, for its hover highlight.
fn hover(h: &mut Harness<'static, FerrodockApp>, label: &str) {
    let at = h.get_by_label(label).rect().center();
    h.event(egui::Event::PointerMoved(at));
    h.run_steps(3);
}

fn pointer_gone(h: &mut Harness<'static, FerrodockApp>) {
    h.event(egui::Event::PointerGone);
    h.run_steps(3);
}

/// The README demo: the container list grouped by project, a container's
/// live logs and stats, the command palette, and Quick start. Assemble the
/// frames with `scripts/demo-gif.py`:
///
/// ```sh
/// SCREENSHOTS=target/screenshots cargo test demo_frames -- --ignored
/// python3 scripts/demo-gif.py target/screenshots/demo docs/screenshots/demo.gif \
///     --webp website/static/img/demo.webp
/// ```
///
/// Looks best with a Compose project running (e.g. nginx with a published
/// port that gets requests, Redis, Postgres) and a container on its own.
#[test]
#[ignore]
fn demo_frames() {
    let mut h = harness();
    settle(&mut h, Duration::from_secs(3));
    set_theme(&mut h, Theme::Light);
    open(&mut h, 0, None);
    click(&mut h, "Group by project");
    // A web server with a published port has the liveliest logs.
    let target = h
        .state()
        .store
        .containers
        .iter()
        .find(|c| c.is_running() && c.ports.iter().any(|p| p.public.is_some()))
        .map(|c| c.id.clone())
        .or_else(|| busiest(&h))
        .expect("a running container");
    let name = h
        .state()
        .store
        .containers
        .iter()
        .find(|c| c.id == target)
        .map(|c| c.name.clone())
        .unwrap();
    // Collect some CPU and memory history, so the graphs aren't empty.
    open(&mut h, 0, Some(target.clone()));
    click(&mut h, "Stats");
    settle(&mut h, Duration::from_secs(8));
    h.state_mut().pages[0].clear_selection();
    pointer_gone(&mut h);
    settle(&mut h, Duration::from_secs(2));

    let mut r = Recorder::new();
    // The list, with live sparklines.
    r.live(&mut h, 2400, 400);

    // Pick a container: its live logs, then its stats.
    hover(&mut h, &name);
    r.frame(&mut h, 500);
    h.state_mut().pages[0].select(&target);
    h.run_steps(3);
    click(&mut h, "Logs");
    pointer_gone(&mut h);
    r.live(&mut h, 2800, 350);
    hover(&mut h, "Stats");
    r.frame(&mut h, 350);
    click(&mut h, "Stats");
    pointer_gone(&mut h);
    r.live(&mut h, 2400, 400);

    // The command palette, jumping to Quick start.
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::K);
    h.run_steps(4);
    r.frame(&mut h, 600);
    for c in "quick".chars() {
        h.event(egui::Event::Text(c.into()));
        h.run_steps(2);
        r.frame(&mut h, 160);
    }
    r.frame(&mut h, 800);
    h.key_press(egui::Key::Enter);
    settle(&mut h, Duration::from_millis(800));
    r.frame(&mut h, 2400);

    // Back to the list.
    hover(&mut h, "Containers");
    r.frame(&mut h, 350);
    h.state_mut().open_page(0);
    h.state_mut().pages[0].clear_selection();
    pointer_gone(&mut h);
    r.live(&mut h, 800, 400);
    r.finish();
}
