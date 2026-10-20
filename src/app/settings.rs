//! User preferences: what they are, how they're applied, and the dialog
//! that changes them. They're saved with the window state between runs.

use eframe::egui::{self, CornerRadius, Frame, Margin, RichText, Stroke, ThemePreference};
use serde::{Deserialize, Serialize};

use super::components::confirm::{self, Choice};
use super::components::{table, widgets};
use super::page::{Confirmation, TypedCheck};
use crate::docker::RESET_WORD;
use crate::theme::Palette;
use crate::update::{Status, Updater};
use crate::vm;

/// Key the settings are stored under.
pub const STORAGE_KEY: &str = "settings";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Theme {
    #[default]
    Light,
    Dark,
    /// Follow the operating system.
    System,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Density {
    #[default]
    Comfortable,
    /// One-line rows, so more fit on screen.
    Compact,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub theme: Theme,
    /// An editor theme (its id) used instead of Ferrodock's own colours;
    /// it brings its own light or dark mode.
    pub editor_theme: Option<String>,
    pub density: Density,
    /// Interface size, 0.9 to 1.25.
    pub scale: f32,
    /// Look for a new version when Ferrodock starts.
    pub check_updates: bool,
    /// Closing the window hides it, leaving Ferrodock in the menu bar
    /// (notification area on Windows).
    pub close_to_tray: bool,
    /// The engine picked last (a Docker context's name, or a detected
    /// engine's); None follows the Docker CLI's current context.
    pub engine: Option<String>,
    /// Compose files brought up from Ferrodock, newest first.
    pub recent_compose: Vec<String>,
    /// Show a system notification when a container crashes or runs out
    /// of memory while Ferrodock isn't in front.
    pub notify_crashes: bool,
    /// Start the Docker engine when Ferrodock opens and finds it down.
    pub start_engine: bool,
    /// The app that served the engine when it last ran (macOS), to start
    /// it again.
    pub engine_app: Option<String>,
    /// CPUs, memory and disk size for Ferrodock's engine (macOS); None is
    /// the default.
    pub engine_cpus: Option<u32>,
    pub engine_memory_mib: Option<u64>,
    pub engine_disk_gib: Option<u64>,
    /// Ferrodock has put `fdock` on the PATH (or found that it needs a
    /// password), so it doesn't try again at start.
    pub cli_installed: bool,
    /// Ferrodock opens when the user logs in. Read from the system at
    /// start, never saved: the system's login items are the record.
    #[serde(skip)]
    pub launch_at_login: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: Theme::default(),
            editor_theme: None,
            density: Density::default(),
            scale: 1.0,
            check_updates: true,
            close_to_tray: true,
            engine: None,
            recent_compose: Vec::new(),
            notify_crashes: true,
            start_engine: true,
            engine_app: None,
            engine_cpus: None,
            engine_memory_mib: None,
            engine_disk_gib: None,
            cli_installed: false,
            launch_at_login: false,
        }
    }
}

impl Settings {
    /// Compose files kept in the recent list.
    const RECENT: usize = 8;

    /// Puts `files` at the top of the recent Compose files.
    pub fn remember_compose_files(&mut self, files: &[std::path::PathBuf]) {
        for file in files.iter().rev() {
            let file = file.display().to_string();
            self.recent_compose.retain(|f| *f != file);
            self.recent_compose.insert(0, file);
        }
        self.recent_compose.truncate(Self::RECENT);
    }

    /// What Ferrodock's engine gets: the chosen CPUs, memory and disk,
    /// within what this computer has.
    pub fn resources(&self) -> vm::Resources {
        let (default, most) = (vm::Resources::default(), vm::Resources::most());
        vm::Resources {
            cpus: self.engine_cpus.unwrap_or(default.cpus).clamp(1, most.cpus),
            memory_mib: self
                .engine_memory_mib
                .unwrap_or(default.memory_mib)
                .clamp(1024, most.memory_mib.max(1024)),
            disk_gib: self
                .engine_disk_gib
                .unwrap_or(default.disk_gib)
                .clamp(vm::DISK_GIB[0], most.disk_gib),
        }
    }

    pub const MIN_SCALE: f32 = 0.9;
    pub const MAX_SCALE: f32 = 1.25;

    /// Makes the settings take effect.
    pub fn apply(&self, ctx: &egui::Context) {
        match self
            .editor_theme
            .as_deref()
            .and_then(crate::theme::editor_theme)
        {
            Some(editor) if editor.dark => {
                crate::theme::use_palettes(ctx, &editor.palette, &crate::theme::LIGHT);
                ctx.set_theme(ThemePreference::Dark);
            }
            Some(editor) => {
                crate::theme::use_palettes(ctx, &crate::theme::DARK, &editor.palette);
                ctx.set_theme(ThemePreference::Light);
            }
            None => {
                crate::theme::use_palettes(ctx, &crate::theme::DARK, &crate::theme::LIGHT);
                ctx.set_theme(match self.theme {
                    Theme::Light => ThemePreference::Light,
                    Theme::Dark => ThemePreference::Dark,
                    Theme::System => ThemePreference::System,
                });
            }
        }
        ctx.set_zoom_factor(self.scale.clamp(Self::MIN_SCALE, Self::MAX_SCALE));
        table::set_compact(ctx, self.density == Density::Compact);
    }
}

/// Draws the Settings dialog. Changes apply immediately. Returns false
/// once it should close.
pub fn show(
    ui: &mut egui::Ui,
    p: &Palette,
    settings: &mut Settings,
    updater: &Updater,
    engine: &vm::Engine,
) -> bool {
    let before = settings.clone();
    let mut close = false;
    let modal = egui::Modal::new(egui::Id::unique("ferrodock-settings"))
        .frame(
            Frame::new()
                .fill(p.sidebar)
                .stroke(Stroke::new(1.0, p.border))
                .corner_radius(CornerRadius::same(10))
                .inner_margin(Margin::same(20)),
        )
        .show(ui.ctx(), |ui| {
            ui.set_width(430.0);
            ui.label(RichText::new("Settings").size(18.0).strong().color(p.text));
            ui.add_space(14.0);

            heading(ui, p, "Appearance");
            egui::Grid::new("settings-grid")
                .num_columns(2)
                .spacing([18.0, 12.0])
                .show(ui, |ui| {
                    ui.label(RichText::new("Theme").color(p.muted));
                    ui.horizontal(|ui| {
                        for (theme, label) in [
                            (Theme::Light, "Light"),
                            (Theme::Dark, "Dark"),
                            (Theme::System, "System"),
                        ] {
                            let on = settings.editor_theme.is_none() && settings.theme == theme;
                            if ui.selectable_label(on, label).clicked() {
                                settings.theme = theme;
                                settings.editor_theme = None;
                            }
                        }
                    });
                    ui.end_row();

                    ui.label(RichText::new("Density").color(p.muted));
                    ui.horizontal(|ui| {
                        ui.selectable_value(
                            &mut settings.density,
                            Density::Comfortable,
                            "Comfortable",
                        );
                        ui.selectable_value(&mut settings.density, Density::Compact, "Compact");
                    });
                    ui.end_row();

                    ui.label(RichText::new("Interface size").color(p.muted));
                    // The slider shows percent; the setting is a factor.
                    let mut percent = (settings.scale * 100.0).round();
                    let slider = egui::Slider::new(
                        &mut percent,
                        Settings::MIN_SCALE * 100.0..=Settings::MAX_SCALE * 100.0,
                    )
                    .step_by(5.0)
                    .suffix("%");
                    // Apply when the drag ends, so the slider doesn't move
                    // under the pointer while the whole window rescales.
                    let response = ui.add(slider);
                    if response.drag_stopped() || (response.changed() && !response.dragged()) {
                        settings.scale = percent / 100.0;
                    }
                    ui.end_row();
                });

            ui.add_space(14.0);
            ui.label(RichText::new("Editor themes").color(p.muted));
            ui.add_space(6.0);
            egui::Grid::new("editor-themes")
                .num_columns(3)
                .spacing([12.0, 10.0])
                .show(ui, |ui| {
                    for (i, editor) in crate::theme::EDITOR_THEMES.iter().enumerate() {
                        let on = settings.editor_theme.as_deref() == Some(editor.id);
                        if theme_swatch(ui, p, editor, on).clicked() {
                            settings.editor_theme = Some(editor.id.to_owned());
                        }
                        if i % 3 == 2 {
                            ui.end_row();
                        }
                    }
                });
            ui.add_space(18.0);
            heading(ui, p, "Notifications");
            ui.checkbox(
                &mut settings.notify_crashes,
                "Tell me when a container crashes, even when Ferrodock is in the background",
            );

            ui.add_space(18.0);
            heading(ui, p, "Startup");
            ui.checkbox(
                &mut settings.start_engine,
                "Start the Docker engine when Ferrodock opens, if it isn't running",
            );
            let before = settings.launch_at_login;
            ui.checkbox(
                &mut settings.launch_at_login,
                "Open Ferrodock when you log in",
            );
            let error_id = egui::Id::unique("launch-at-login-error");
            if settings.launch_at_login != before
                && let Err(e) = crate::app::autostart::set(settings.launch_at_login)
            {
                settings.launch_at_login = before;
                ui.data_mut(|d| d.insert_temp(error_id, e));
            }
            if let Some(error) = ui.data(|d| d.get_temp::<String>(error_id)) {
                ui.label(RichText::new(format!("Couldn't change it: {error}")).color(p.danger));
            }
            if vm::SUPPORTED && engine.state() != vm::State::NotInstalled {
                ui.add_space(18.0);
                heading(ui, p, "Ferrodock's engine");
                engine_section(ui, p, settings, engine);
            }
            if crate::app::tray::Tray::SUPPORTED {
                ui.add_space(18.0);
                let place = if cfg!(target_os = "macos") {
                    "Menu bar"
                } else {
                    "Notification area"
                };
                heading(ui, p, place);
                ui.checkbox(
                    &mut settings.close_to_tray,
                    "Keep running there when the window is closed",
                );
            }

            ui.add_space(18.0);
            heading(ui, p, "Updates");
            ui.checkbox(
                &mut settings.check_updates,
                "Check for updates when Ferrodock starts",
            );
            ui.horizontal(|ui| {
                let status = updater.status();
                let (text, color) = match &status {
                    Status::Idle => (String::new(), p.muted),
                    Status::Checking => ("Checking…".into(), p.muted),
                    Status::UpToDate => ("Ferrodock is up to date.".into(), p.muted),
                    Status::Available(a) => {
                        (format!("Version {} is available.", a.version), p.text)
                    }
                    Status::Downloading { version, .. } => {
                        (format!("Downloading {version}…"), p.muted)
                    }
                    Status::Ready { version, .. } => (
                        format!("Version {version} is installed. Restart to use it."),
                        p.text,
                    ),
                    Status::Failed(e) => (e.clone(), p.danger),
                };
                if let Status::Available(available) = status {
                    if widgets::primary_button(ui, "Update now").clicked() {
                        updater.install(available);
                    }
                } else if ui.button("Check now").clicked() {
                    updater.check();
                }
                ui.label(RichText::new(text).color(color));
            });
            if crate::update::installed_with_homebrew() {
                ui.label(
                    RichText::new(
                        "Installed with Homebrew: `brew upgrade --cask ferrodock` works too.",
                    )
                    .size(12.0)
                    .color(p.muted),
                );
            }

            ui.add_space(18.0);
            heading(ui, p, "About");
            ui.label(
                RichText::new(format!("Ferrodock {}", env!("CARGO_PKG_VERSION"))).color(p.text),
            );
            ui.horizontal(|ui| {
                ui.hyperlink_to("Website", "https://manimovassagh.github.io/Ferrodock/");
                ui.label(RichText::new("·").color(p.muted));
                ui.hyperlink_to("GitHub", "https://github.com/manimovassagh/Ferrodock");
                ui.label(RichText::new("·").color(p.muted));
                ui.hyperlink_to(
                    "Report a problem",
                    "https://github.com/manimovassagh/Ferrodock/issues/new/choose",
                );
            });

            ui.add_space(18.0);
            ui.horizontal(|ui| {
                if ui.button("Reset to defaults").clicked() {
                    *settings = Settings::default();
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    close = ui.button("Done").clicked();
                });
            });
        });
    if *settings != before {
        settings.apply(ui.ctx());
    }
    let open = !(close || modal.should_close());
    reset_dialog(ui, p, settings, engine, open);
    open
}

/// Where the open "Reset engine?" dialog keeps its state.
fn reset_id() -> egui::Id {
    egui::Id::unique("ferrodock-engine-reset")
}

/// Asks before resetting the engine, over the Settings dialog, and
/// resets it on a yes. Closing Settings drops the question.
fn reset_dialog(
    ui: &mut egui::Ui,
    p: &Palette,
    settings: &Settings,
    engine: &vm::Engine,
    open: bool,
) {
    let Some(mut confirmation) = ui.data(|d| d.get_temp::<Confirmation>(reset_id())) else {
        return;
    };
    let choice = if open {
        confirm::show(ui, p, &mut confirmation)
    } else {
        Some(Choice::Cancel)
    };
    match choice {
        Some(Choice::Confirm) => {
            ui.data_mut(|d| d.remove::<Confirmation>(reset_id()));
            engine.reset(settings.resources(), repaint(ui));
        }
        Some(Choice::Cancel) => ui.data_mut(|d| d.remove::<Confirmation>(reset_id())),
        // Keep what was typed so far.
        None => {
            ui.data_mut(|d| d.insert_temp(reset_id(), confirmation));
        }
    }
}

/// The "Reset engine?" question: what's deleted, said plainly.
fn reset_confirmation() -> Confirmation {
    let how = if cfg!(windows) {
        "Its WSL distro, ferrodock, is removed and set up again, which downloads the engine \
         again."
    } else if cfg!(target_os = "linux") {
        "Its data folder, /var/lib/ferrodock, is deleted. This asks for your admin password."
    } else {
        "Its disk, ~/.ferrodock/data.img, is deleted and made again, empty, at the size \
         chosen above."
    };
    Confirmation {
        title: "Reset Ferrodock's engine?".into(),
        message: format!(
            "This stops the engine and deletes everything in it: all containers, images, \
             volumes (and the data in them), networks and build cache. Then it starts again, \
             empty, as on first use.\n\n{how}\n\nThis can't be undone."
        ),
        action_label: "Reset engine".into(),
        typed: Some(TypedCheck::new(RESET_WORD)),
        ..Default::default()
    }
}

/// Wakes the UI as the engine's state changes.
fn repaint(ui: &egui::Ui) -> impl Fn() + Send + Sync + 'static {
    let ctx = ui.ctx().clone();
    move || ctx.request_repaint()
}

/// Ferrodock's own engine: how it's doing, what it gets (macOS), start,
/// stop, restart and reset, and its log.
fn engine_section(ui: &mut egui::Ui, p: &Palette, settings: &mut Settings, engine: &vm::Engine) {
    let state = engine.state();
    let (text, color) = match &state {
        vm::State::Running => ("Running".to_owned(), p.text),
        vm::State::Stopped | vm::State::NotInstalled => ("Stopped".to_owned(), p.muted),
        vm::State::InstallingWsl => ("Installing WSL…".to_owned(), p.muted),
        vm::State::NeedsRestart => ("Restart Windows to finish setting up".to_owned(), p.muted),
        vm::State::Starting => ("Starting…".to_owned(), p.muted),
        vm::State::Stopping => ("Stopping…".to_owned(), p.muted),
        vm::State::Downloading { update: false, .. } => ("Downloading…".to_owned(), p.muted),
        vm::State::Downloading { update: true, .. } => ("Updating…".to_owned(), p.muted),
        vm::State::Failed(e) => (e.clone(), p.danger),
    };
    ui.label(RichText::new(text).color(color));
    if cfg!(target_os = "macos") {
        let (default, most) = (vm::Resources::default(), vm::Resources::most());
        let chosen = settings.resources();
        egui::Grid::new("engine-grid")
            .num_columns(2)
            .spacing([18.0, 12.0])
            .show(ui, |ui| {
                ui.label(RichText::new("CPUs").color(p.muted));
                let mut cpus = chosen.cpus;
                if ui
                    .add(egui::Slider::new(&mut cpus, 1..=most.cpus))
                    .changed()
                {
                    settings.engine_cpus = (cpus != default.cpus).then_some(cpus);
                }
                ui.end_row();

                ui.label(RichText::new("Memory").color(p.muted));
                let mut gib = chosen.memory_mib / 1024;
                let slider =
                    egui::Slider::new(&mut gib, 1..=(most.memory_mib / 1024).max(1)).suffix(" GB");
                if ui.add(slider).changed() {
                    let mib = gib * 1024;
                    settings.engine_memory_mib = (mib != default.memory_mib).then_some(mib);
                }
                ui.end_row();

                ui.label(RichText::new("Disk").color(p.muted));
                ui.horizontal(|ui| {
                    for gib in vm::DISK_GIB {
                        if ui
                            .selectable_label(chosen.disk_gib == gib, format!("{gib} GB"))
                            .clicked()
                        {
                            settings.engine_disk_gib = (gib != default.disk_gib).then_some(gib);
                        }
                    }
                });
                ui.end_row();
            });
        // The disk never shrinks: that would lose what's on it.
        let note = match engine.disk_gib() {
            Some(now) if chosen.disk_gib < now => Some(format!(
                "The disk is {now} GB and only grows: reset the engine to start over with a \
                 smaller one."
            )),
            _ if engine.resources().is_some_and(|given| given != chosen) => {
                Some("Restart the engine to use the new settings.".to_owned())
            }
            _ => None,
        };
        if let Some(note) = note {
            ui.label(RichText::new(note).size(12.0).color(p.muted));
        }
    }
    ui.horizontal(|ui| {
        match state {
            vm::State::Running => {
                if ui.button("Stop engine").clicked() {
                    engine.shut_down(repaint(ui));
                }
                if vm::Engine::RESTARTS && ui.button("Restart engine").clicked() {
                    engine.restart(settings.resources(), repaint(ui));
                }
            }
            vm::State::Stopped | vm::State::NotInstalled | vm::State::Failed(_) => {
                if ui.button("Start engine").clicked() {
                    engine.start(settings.resources(), repaint(ui));
                }
            }
            // Busy, or waiting for Windows to restart: nothing to click.
            vm::State::InstallingWsl
            | vm::State::NeedsRestart
            | vm::State::Starting
            | vm::State::Stopping
            | vm::State::Downloading { .. } => {}
        }
        if !state.busy() && ui.button("Reset engine…").clicked() {
            ui.data_mut(|d| d.insert_temp(reset_id(), reset_confirmation()));
        }
        if let Some(log) = engine.log()
            && ui.button("Open log").clicked()
        {
            open_file(&log);
        }
    });
    if cfg!(target_os = "linux") {
        ui.label(
            RichText::new(
                "It runs as the ferrodock-engine service. Its log: journalctl -u ferrodock-engine",
            )
            .size(12.0)
            .color(p.muted),
        );
    }
}

/// Opens a text file in the system's editor.
fn open_file(path: &std::path::Path) {
    let mut command = if cfg!(target_os = "macos") {
        let mut c = std::process::Command::new("open");
        c.arg("-t");
        c
    } else if cfg!(windows) {
        std::process::Command::new("notepad.exe")
    } else {
        std::process::Command::new("xdg-open")
    };
    let _ = command.arg(path).spawn();
}

/// An editor theme to pick: a small preview of its colours (background,
/// text lines in its text and accent colours) and its name under it.
fn theme_swatch(
    ui: &mut egui::Ui,
    p: &Palette,
    editor: &crate::theme::EditorTheme,
    selected: bool,
) -> egui::Response {
    let size = egui::vec2(132.0, 70.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    let painter = ui.painter();
    let theme = &editor.palette;
    let preview = egui::Rect::from_min_size(rect.min, egui::vec2(size.x, 48.0));
    painter.rect_filled(preview, CornerRadius::same(6), theme.bg);
    let side = egui::Rect::from_min_size(preview.min, egui::vec2(26.0, preview.height()));
    painter.rect_filled(
        side,
        CornerRadius {
            nw: 6,
            sw: 6,
            ne: 0,
            se: 0,
        },
        theme.sidebar,
    );
    for (row, (color, width)) in [
        (theme.accent, 34.0),
        (theme.text, 50.0),
        (theme.muted, 40.0),
    ]
    .into_iter()
    .enumerate()
    {
        let y = preview.top() + 12.0 + row as f32 * 11.0;
        let line =
            egui::Rect::from_min_size(egui::pos2(side.right() + 8.0, y), egui::vec2(width, 4.0));
        painter.rect_filled(line, CornerRadius::same(2), color);
    }
    painter.circle_filled(side.center_top() + egui::vec2(0.0, 12.0), 4.0, theme.accent);
    let ring = if selected {
        Stroke::new(2.0, p.accent)
    } else if response.hovered() {
        Stroke::new(1.0, p.muted)
    } else {
        Stroke::new(1.0, p.border)
    };
    painter.rect_stroke(
        preview,
        CornerRadius::same(6),
        ring,
        egui::StrokeKind::Outside,
    );
    painter.text(
        egui::pos2(rect.center().x, preview.bottom() + 12.0),
        egui::Align2::CENTER_CENTER,
        editor.name,
        egui::FontId::proportional(11.5),
        if selected { p.text } else { p.muted },
    );
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::Role::RadioButton, true, selected, editor.name)
    });
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn heading(ui: &mut egui::Ui, p: &Palette, text: &str) {
    ui.label(RichText::new(text.to_uppercase()).size(11.0).color(p.muted));
    ui.add_space(6.0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn light_is_the_default() {
        let s = Settings::default();
        assert_eq!(s.theme, Theme::Light);
        assert_eq!(s.density, Density::Comfortable);
        assert_eq!(s.scale, 1.0);
    }

    #[test]
    fn missing_fields_fall_back_to_defaults() {
        // Settings saved by an older version may lack newer fields.
        let s: Settings = serde_json::from_str(r#"{"theme":"Dark"}"#).unwrap();
        assert_eq!(s.theme, Theme::Dark);
        assert_eq!(s.scale, 1.0);
    }

    #[test]
    fn engine_gets_what_was_chosen_within_limits() {
        let mut s = Settings::default();
        assert_eq!(s.resources(), vm::Resources::default());
        s.engine_cpus = Some(10_000);
        s.engine_memory_mib = Some(1);
        s.engine_disk_gib = Some(1);
        let r = s.resources();
        assert_eq!(r.cpus, vm::Resources::most().cpus);
        assert_eq!(r.memory_mib, 1024);
        assert_eq!(r.disk_gib, vm::DISK_GIB[0]);
        let old: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(old.engine_cpus, None);
        assert_eq!(old.resources().disk_gib, 128);
    }
}
