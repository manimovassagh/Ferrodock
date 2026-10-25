//! Left sidebar: brand, page navigation, the settings and activity
//! buttons and Docker engine status.

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
    /// The log of what happened and what went wrong.
    Activity,
    /// Download and install the available update.
    Update,
    /// Restart into the installed update.
    Restart,
    /// Switch to another Docker engine.
    Engine(crate::docker::Engine),
    /// How to use Ferrodock from a terminal and AI tools.
    Integrations,
}

/// Draws the sidebar.
pub fn show(
    ui: &mut egui::Ui,
    p: &Palette,
    pages: &[Box<dyn Page>],
    active: usize,
    store: &Store,
    update: &Status,
    unread_errors: &str,
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
    let mut clicked = None;

    // The bottom half first, from the bottom up, so the pages above know
    // how much room is left: in a short window they scroll rather than
    // run under it.
    let space = ui.available_rect_before_wrap();
    let mut bottom_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(space)
            .layout(Layout::bottom_up(Align::Min)),
    );
    bottom(
        &mut bottom_ui,
        p,
        pages,
        active,
        store,
        &Footer {
            update,
            unread_errors,
        },
        &mut clicked,
    );
    let left = space.height() - bottom_ui.min_rect().height() - 8.0;

    // A solid scroll bar, so pages out of view are seen to be there.
    ui.style_mut().spacing.scroll = egui::style::ScrollStyle::solid();
    egui::ScrollArea::vertical()
        .id_salt("sidebar-pages")
        .max_height(left.max(0.0))
        .auto_shrink([false, true])
        .show(ui, |ui| {
            ui.label(RichText::new("DOCKER").size(11.0).color(p.muted));
            ui.add_space(2.0);
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
        });
    clicked
}

/// What the bottom half shows besides pages: an update, and errors not
/// yet seen in the activity log.
struct Footer<'a> {
    update: &'a Status,
    unread_errors: &'a str,
}

/// Below this height (in points, under the logo) the sidebar is short.
const SHORT: f32 = 560.0;

/// The sidebar's bottom half: the Start pages, CLI & MCP, Settings, an
/// update and the engine. Drawn bottom-up.
fn bottom(
    ui: &mut egui::Ui,
    p: &Palette,
    pages: &[Box<dyn Page>],
    active: usize,
    store: &Store,
    footer: &Footer<'_>,
    clicked: &mut Option<Click>,
) {
    let Footer {
        update,
        unread_errors,
    } = *footer;
    // Short windows get a one-line CLI & MCP, leaving more room for
    // the pages above.
    let compact = ui.available_height() < SHORT;
    if let Some(engine) = engine_picker(ui, p, store) {
        *clicked = Some(Click::Engine(engine));
    }
    ui.add_space(6.0);
    let hint = if unread_errors.is_empty() {
        "What Ferrodock did and what went wrong".to_owned()
    } else {
        format!("{unread_errors} new errors")
    };
    let settings_hint = format!(
        "Theme, density and interface size ({}{})",
        super::keys::COMMAND,
        ","
    );
    if compact {
        // One row for both, so the pages above keep their room: the
        // activity log as an icon, dotted red when errors are unread.
        let width = ui.available_width();
        ui.allocate_ui_with_layout(
            egui::vec2(width, 32.0),
            Layout::right_to_left(Align::Center),
            |ui| {
                let activity = widgets::icon_button(ui, Icon::Activity, &hint);
                if !unread_errors.is_empty() {
                    let dot = activity.rect.right_top() + egui::vec2(-6.0, 6.0);
                    ui.painter().circle_filled(dot, 3.5, p.danger);
                }
                if activity.clicked() {
                    *clicked = Some(Click::Activity);
                }
                // The rest of the row, drawn left to right again.
                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                    if widgets::nav_item(ui, p, Icon::Gear, "Settings", "", false)
                        .on_hover_text(&settings_hint)
                        .clicked()
                    {
                        *clicked = Some(Click::Settings);
                    }
                });
            },
        );
    } else {
        if widgets::nav_item(ui, p, Icon::Gear, "Settings", "", false)
            .on_hover_text(&settings_hint)
            .clicked()
        {
            *clicked = Some(Click::Settings);
        }
        if widgets::nav_item(ui, p, Icon::Activity, "Activity", unread_errors, false)
            .on_hover_text(&hint)
            .clicked()
        {
            *clicked = Some(Click::Activity);
        }
    }
    if let Some(click) = update_item(ui, p, update) {
        *clicked = Some(click);
    }
    ui.add_space(if compact { 2.0 } else { 8.0 });
    let card = if compact {
        widgets::nav_item(ui, p, Icon::Terminal, "CLI & MCP", "", false)
    } else {
        widgets::feature_card(ui, p, Icon::Terminal, "CLI & MCP", "Terminal and AI tools")
    }
    .on_hover_text("Use Ferrodock from a terminal (fdock) and from AI tools (MCP)");
    if card.clicked() {
        *clicked = Some(Click::Integrations);
    }
    // Drawn bottom-up: the pages, then their heading above them.
    let bottom: Vec<(usize, &Box<dyn Page>)> = pages
        .iter()
        .enumerate()
        .filter(|(_, page)| page.bottom())
        .collect();
    if !bottom.is_empty() {
        ui.add_space(if compact { 6.0 } else { 14.0 });
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
                *clicked = Some(Click::Page(*i));
            }
        }
        ui.add_space(2.0);
        ui.label(RichText::new("START").size(11.0).color(p.muted));
    }
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
