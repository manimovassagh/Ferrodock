//! Left sidebar: brand, page navigation, the settings button and Docker
//! engine status.

use eframe::egui::{self, Align, Layout, RichText};

use super::components::icons::Icon;
use super::components::widgets;
use super::page::Page;
use super::store::{Connection, Store};
use crate::theme::Palette;
use crate::update::Status;

/// What the user clicked in the sidebar.
pub enum Click {
    Page(usize),
    Settings,
    /// Download and install the available update.
    Update,
    /// Restart into the installed update.
    Restart,
    /// Switch to another Docker engine.
    Engine(crate::docker::Engine),
}

/// Draws the sidebar.
pub fn show(
    ui: &mut egui::Ui,
    p: &Palette,
    pages: &[Box<dyn Page>],
    active: usize,
    store: &Store,
    update: &Status,
) -> Option<Click> {
    ui.add_space(2.0);
    ui.horizontal(|ui| {
        widgets::logo(ui, 26.0);
        ui.label(
            RichText::new("Ferrodock")
                .font(egui::FontId::new(16.0, crate::theme::semibold()))
                .color(p.text),
        );
    });

    ui.add_space(22.0);
    ui.label(RichText::new("DOCKER").size(11.0).color(p.muted));
    ui.add_space(2.0);
    let mut clicked = None;
    for (i, page) in pages.iter().enumerate().filter(|(_, page)| !page.bottom()) {
        let item = widgets::nav_item(
            ui,
            p,
            page.icon(),
            page.title(),
            &page.badge(store),
            i == active,
        );
        if item.clicked() {
            clicked = Some(Click::Page(i));
        }
    }

    ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
        if let Some(engine) = engine_picker(ui, p, store) {
            clicked = Some(Click::Engine(engine));
        }
        ui.add_space(6.0);
        let settings =
            widgets::nav_item(ui, p, Icon::Gear, "Settings", "", false).on_hover_text(format!(
                "Theme, density and interface size ({}{})",
                super::keys::COMMAND,
                ","
            ));
        if settings.clicked() {
            clicked = Some(Click::Settings);
        }
        if let Some(click) = update_item(ui, p, update) {
            clicked = Some(click);
        }
        // Drawn bottom-up: the pages, then their heading above them.
        let bottom: Vec<(usize, &Box<dyn Page>)> = pages
            .iter()
            .enumerate()
            .filter(|(_, page)| page.bottom())
            .collect();
        if !bottom.is_empty() {
            ui.add_space(14.0);
            for (i, page) in bottom.iter().rev() {
                let item = widgets::nav_item(
                    ui,
                    p,
                    page.icon(),
                    page.title(),
                    &page.badge(store),
                    *i == active,
                );
                if item.clicked() {
                    clicked = Some(Click::Page(*i));
                }
            }
            ui.add_space(2.0);
            ui.label(RichText::new("START").size(11.0).color(p.muted));
        }
    });
    clicked
}

/// Offers an available update, shows its progress, then a restart.
fn update_item(ui: &mut egui::Ui, p: &Palette, update: &Status) -> Option<Click> {
    match update {
        Status::Available(available) => {
            let mut notes: String = available.notes.chars().take(600).collect();
            if notes.len() < available.notes.len() {
                notes.push('…');
            }
            let item = widgets::nav_item(
                ui,
                p,
                Icon::Download,
                &format!("Update to {}", available.version),
                "",
                true,
            )
            .on_hover_text(if notes.is_empty() {
                "A new version is available".into()
            } else {
                notes
            });
            item.clicked().then_some(Click::Update)
        }
        Status::Downloading { done, total, .. } => {
            let fraction = total
                .filter(|t| *t > 0)
                .map_or(0.0, |t| *done as f32 / t as f32);
            ui.add(
                egui::ProgressBar::new(fraction)
                    .desired_height(4.0)
                    .fill(p.accent),
            );
            ui.label(RichText::new("Updating…").size(12.0).color(p.muted));
            None
        }
        Status::Ready { version, .. } => {
            let item = widgets::nav_item(ui, p, Icon::Restart, "Restart to update", "", true)
                .on_hover_text(format!("Version {version} is installed"));
            item.clicked().then_some(Click::Restart)
        }
        _ => None,
    }
}

/// The engine's status; clicking it lists the engines to switch to.
fn engine_picker(ui: &mut egui::Ui, p: &Palette, store: &Store) -> Option<crate::docker::Engine> {
    let on = match store.engine.as_deref() {
        Some(name) if name != "default" => format!(" · {name}"),
        _ => String::new(),
    };
    let (dot, text) = match &store.connection {
        Connection::Connecting => (None, "Connecting…".to_owned()),
        Connection::Connected { version } => (Some(p.success), format!("Docker {version}{on}")),
        Connection::Disconnected { .. } if on.is_empty() => {
            (Some(p.danger), "Docker not running".to_owned())
        }
        Connection::Disconnected { .. } => (Some(p.danger), format!("Not running{on}")),
    };
    let mut picked = None;
    ui.horizontal(|ui| {
        match dot {
            Some(color) => widgets::dot(ui, color),
            None => {
                ui.spinner();
            }
        }
        let row = ui
            .add(egui::Button::new(RichText::new(text).color(p.muted)).frame(false))
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text("Switch Docker engine");
        row.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, "Docker engine"));
        egui::Popup::menu(&row).show(|ui| {
            ui.set_min_width(240.0);
            ui.label(RichText::new("DOCKER ENGINES").size(11.0).color(p.muted));
            for engine in &store.engines {
                let active = store.engine.as_deref() == Some(engine.name.as_str());
                let label = if engine.current && engine.name != "default" {
                    format!("{} (current context)", engine.name)
                } else {
                    engine.name.clone()
                };
                let item = ui
                    .add_enabled(engine.supported(), egui::Button::selectable(active, label))
                    .on_hover_text(&engine.host)
                    .on_disabled_hover_text(format!(
                        "{} isn't supported on this system",
                        engine.host
                    ));
                if item.clicked() && !active {
                    picked = Some(engine.clone());
                }
            }
        });
    });
    picked
}
