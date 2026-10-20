// Hide the console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
/// The Docker layer, shared with the CLI and the MCP server.
use ferrodock_core as docker;
mod instance;
mod theme;
mod update;
mod vm;
mod worker;

use eframe::egui;

fn main() -> eframe::Result {
    // Held until Ferrodock exits, after it stopped its engine.
    let instance = match instance::start(app::started_in_background()) {
        instance::Start::Run(instance) => instance,
        instance::Start::Exit => return Ok(()),
    };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Ferrodock")
            .with_inner_size([1240.0, 740.0])
            .with_min_inner_size([820.0, 480.0])
            .with_icon(
                eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon-256.png"))
                    .expect("bundled icon is a valid PNG"),
            ),
        ..Default::default()
    };
    eframe::run_native(
        "Ferrodock",
        options,
        Box::new(|cc| {
            if let Some(instance) = &instance {
                let ctx = cc.egui_ctx.clone();
                // Not being reachable only means a second Ferrodock can't
                // bring this one forward.
                let _ = instance.serve(move || app::show_window(&ctx));
            }
            Ok(Box::new(app::FerrodockApp::new(cc)))
        }),
    )
    // Returning ends the process, along with any threads still running
    // (the engine's socket, port forwarding, the worker), and lets go of
    // the lock for the next Ferrodock.
}
