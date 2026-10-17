//! The tray icon itself, through `tray-icon` (macOS and Windows).

use std::sync::mpsc;

use eframe::egui;
use tray_icon::menu::{IsMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

use super::Entry;

pub struct NativeTray {
    icon: TrayIcon,
    clicks: mpsc::Receiver<String>,
}

impl NativeTray {
    pub fn new(ctx: &egui::Context) -> Result<Self, String> {
        let (tx, clicks) = mpsc::channel();
        let ctx = ctx.clone();
        // Runs on the UI thread, possibly while the window is hidden; the
        // repaint makes eframe run a frame, which handles the click.
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            let _ = tx.send(event.id.0);
            ctx.request_repaint();
        }));
        let png = eframe::icon_data::from_png_bytes(include_bytes!("../../../assets/icon-256.png"))
            .map_err(|e| e.to_string())?;
        let icon = Icon::from_rgba(png.rgba, png.width, png.height).map_err(|e| e.to_string())?;
        let icon = TrayIconBuilder::new()
            .with_icon(icon)
            .with_tooltip("Ferrodock")
            .with_menu(Box::new(Menu::new()))
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Self { icon, clicks })
    }

    pub fn set_menu(&self, entries: &[Entry]) {
        let menu = Menu::new();
        for entry in entries {
            let _ = menu.append(build(entry).as_ref());
        }
        self.icon.set_menu(Some(Box::new(menu)));
    }

    /// The running count: next to the icon on macOS, in the tooltip on
    /// both.
    pub fn set_title(&self, title: &str) {
        #[cfg(target_os = "macos")]
        self.icon.set_title(Some(title));
        let _ = self
            .icon
            .set_tooltip(Some(format!("Ferrodock · {title} running")));
    }

    /// The ids of the items clicked since the last call.
    pub fn clicked(&self) -> Vec<String> {
        self.clicks.try_iter().collect()
    }
}

fn build(entry: &Entry) -> Box<dyn IsMenuItem> {
    match entry {
        Entry::Item { id, label, enabled } => {
            Box::new(MenuItem::with_id(id.as_str(), label, *enabled, None))
        }
        Entry::Separator => Box::new(PredefinedMenuItem::separator()),
        Entry::Menu { label, items } => {
            let submenu = Submenu::new(label, true);
            for item in items {
                let _ = submenu.append(build(item).as_ref());
            }
            Box::new(submenu)
        }
    }
}
