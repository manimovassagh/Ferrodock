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
    Harness::builder()
        .with_size(SIZE)
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
