//! The pages listed in the sidebar. To add one, implement [`Page`] in a new
//! file and register it in [`all`].

mod containers;
mod images;
mod networks;
mod volumes;

use eframe::egui;

use super::components::widgets;
use super::page::Page;
use crate::theme::Palette;

/// Every page, in sidebar order.
pub fn all() -> Vec<Box<dyn Page>> {
    vec![
        Box::new(containers::ContainersPage::new()),
        Box::new(images::ImagesPage::default()),
        Box::new(volumes::VolumesPage::default()),
        Box::new(networks::NetworksPage::default()),
    ]
}

/// "In use" / "Unused" badge for images and volumes.
fn usage_pill(ui: &mut egui::Ui, p: &Palette, users: usize) {
    if users > 0 {
        widgets::pill(ui, "In use", p.success);
    } else {
        widgets::pill(ui, "Unused", p.muted);
    }
}
