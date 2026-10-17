// Hide the console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod docker;
mod theme;
mod worker;

use eframe::egui;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("IronStack")
            .with_inner_size([1240.0, 740.0])
            .with_min_inner_size([820.0, 480.0]),
        ..Default::default()
    };
    eframe::run_native(
        "IronStack",
        options,
        Box::new(|cc| Ok(Box::new(app::IronStackApp::new(cc)))),
    )
}
