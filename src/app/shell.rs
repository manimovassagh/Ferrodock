//! The parts of the main area every page shares: the header and the
//! "Docker isn't running" screen.

use eframe::egui::{self, Align, Layout, RichText};

use super::components::icons::{self, Icon};
use super::components::widgets;
use crate::theme::Palette;

/// The search box, so `/` can focus it.
pub fn search_id() -> egui::Id {
    egui::Id::unique("ferrodock-search")
}

/// Page title, summary line, the page's own toolbar, search box and
/// refresh button.
/// Returns true when refresh was clicked.
pub fn header(
    ui: &mut egui::Ui,
    p: &Palette,
    title: &str,
    summary: &str,
    search_hint: &str,
    search: &mut String,
    toolbar: impl FnOnce(&mut egui::Ui),
) -> bool {
    let mut refresh = false;
    // With room to spare, the page's buttons sit next to the search box;
    // otherwise they get their own row under the title.
    let wide = ui.available_width() >= 900.0;
    let mut toolbar = Some(toolbar);
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label(
                RichText::new(title)
                    .font(egui::FontId::new(24.0, crate::theme::semibold()))
                    .color(p.text),
            );
            ui.label(RichText::new(summary).color(p.muted));
        });
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            refresh = widgets::icon_button(ui, Icon::Refresh, "Refresh (F5)").clicked();
            search_box(ui, p, search_hint, search);
            if wide && let Some(toolbar) = toolbar.take() {
                toolbar(ui);
            }
        });
    });
    if let Some(toolbar) = toolbar {
        ui.add_space(6.0);
        ui.horizontal(toolbar);
    }
    refresh
}

/// A rounded search field with a magnifier icon.
fn search_box(ui: &mut egui::Ui, p: &Palette, hint: &str, search: &mut String) {
    egui::Frame::new()
        .fill(p.raised)
        .corner_radius(egui::CornerRadius::same(8))
        .inner_margin(egui::Margin::symmetric(9, 0))
        .show(ui, |ui| {
            ui.set_width(240.0);
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                icons::show(ui, Icon::Search, 14.0, p.muted);
                ui.add(
                    egui::TextEdit::singleline(search)
                        .id(search_id())
                        .hint_text(hint)
                        .frame(egui::Frame::NONE)
                        .desired_width(220.0)
                        .margin(egui::vec2(2.0, 5.0)),
                )
                .on_hover_text(format!(
                    "Press / to search this page, {}K to search everything",
                    super::keys::COMMAND
                ));
            });
        });
}

/// Why Ferrodock can't reach Docker, as far as the error message tells.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Problem {
    /// The socket exists but this user may not use it (Linux).
    PermissionDenied,
    /// Nothing is listening: Docker isn't installed or isn't started.
    NotRunning,
}

/// Whether `reason` means nothing is listening (rather than a permission
/// problem), so starting the engine could help.
pub fn is_not_running(reason: &str) -> bool {
    diagnose(reason) == Problem::NotRunning
}

/// What the user clicked on the "Docker isn't running" screen.
pub enum Click {
    Retry,
    Start,
}

fn diagnose(reason: &str) -> Problem {
    let reason = reason.to_lowercase();
    if reason.contains("permission denied") || reason.contains("access is denied") {
        Problem::PermissionDenied
    } else {
        Problem::NotRunning
    }
}

/// Commands that fix `problem` on this platform: (what it does, command).
fn fixes(problem: Problem) -> Vec<(&'static str, &'static str)> {
    match problem {
        Problem::PermissionDenied => vec![(
            "Let your user use Docker, then log out and back in",
            "sudo usermod -aG docker $USER",
        )],
        // Ferrodock starts the engine itself where it can (the Start button).
        Problem::NotRunning if cfg!(target_os = "linux") => {
            vec![("Start the Docker service", "sudo systemctl start docker")]
        }
        Problem::NotRunning => Vec::new(),
    }
}

/// A connecting spinner, shown until the first answer from Docker.
pub fn connecting(ui: &mut egui::Ui, p: &Palette) {
    ui.add_space(120.0);
    ui.vertical_centered(|ui| {
        ui.spinner();
        ui.add_space(8.0);
        ui.label(RichText::new("Connecting to Docker…").color(p.muted));
    });
}

/// Shown instead of the page while the engine is unreachable: what's
/// wrong, how to fix it, and that Ferrodock reconnects by itself.
/// Returns true when "Retry now" was clicked.
pub fn disconnected(
    ui: &mut egui::Ui,
    p: &Palette,
    reason: &str,
    offer: &super::engine_start::Offer,
) -> Option<Click> {
    use super::engine_start::Offer;
    let problem = diagnose(reason);
    let starting = matches!(offer, Offer::Starting(_)) && problem == Problem::NotRunning;
    let starting_title = match offer {
        Offer::Starting(name) => format!("Starting {name}…"),
        _ => String::new(),
    };
    let (title, explanation) = match problem {
        Problem::PermissionDenied => (
            "Docker won't let Ferrodock in",
            "Docker is running, but your user isn't allowed to use it.",
        ),
        Problem::NotRunning if starting => (
            starting_title.as_str(),
            "Ferrodock connects as soon as the engine is up. The first start can take a minute.",
        ),
        Problem::NotRunning if matches!(offer, Offer::Nothing) => (
            "No Docker engine found",
            "Ferrodock couldn't find a Docker engine on this machine to start. \
             Once one runs, Ferrodock connects by itself.",
        ),
        Problem::NotRunning => (
            "Docker isn't running",
            "Start the engine and Ferrodock connects by itself.",
        ),
    };
    let mut click = None;
    ui.add_space(48.0);
    ui.vertical_centered(|ui| {
        ui.set_max_width(520.0);
        let (rect, _) = ui.allocate_exact_size(egui::vec2(64.0, 64.0), egui::Sense::hover());
        ui.painter().rect_filled(
            rect,
            egui::CornerRadius::same(18),
            p.warning.gamma_multiply(0.12),
        );
        icons::paint(ui.painter(), rect.shrink(18.0), Icon::Container, p.warning);
        ui.add_space(14.0);
        ui.label(
            RichText::new(title)
                .font(egui::FontId::new(19.0, crate::theme::semibold()))
                .color(p.text),
        );
        ui.add_space(6.0);
        ui.label(RichText::new(explanation).color(p.muted));
        ui.add_space(16.0);

        if problem == Problem::NotRunning {
            match offer {
                Offer::Start(name) => {
                    if widgets::primary_button(ui, &format!("Start {name}")).clicked() {
                        click = Some(Click::Start);
                    }
                    ui.add_space(16.0);
                }
                Offer::Starting(_) => {
                    ui.spinner();
                    ui.add_space(16.0);
                }
                Offer::Failed { name, error } => {
                    ui.label(RichText::new(error).color(p.danger));
                    ui.add_space(6.0);
                    if widgets::primary_button(ui, &format!("Try starting {name} again")).clicked()
                    {
                        click = Some(Click::Start);
                    }
                    ui.add_space(16.0);
                }
                Offer::Nothing => {}
            }
        }

        if starting {
            return;
        }
        for (what, command) in fixes(problem) {
            egui::Frame::new()
                .fill(p.sidebar)
                .stroke(egui::Stroke::new(1.0, p.border))
                .corner_radius(egui::CornerRadius::same(10))
                .inner_margin(egui::Margin::same(12))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(RichText::new(what).size(12.0).color(p.muted));
                            ui.label(RichText::new(command).monospace().color(p.text));
                        });
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.small_button("Copy").clicked() {
                                ui.ctx().copy_text(command.to_owned());
                            }
                        });
                    });
                });
            ui.add_space(8.0);
        }

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(RichText::new("Trying again every few seconds…").color(p.muted));
        });
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            if ui.button("Retry now").clicked() {
                click = Some(Click::Retry);
            }
            ui.hyperlink_to(
                "Set up an engine",
                "https://manimovassagh.github.io/Ferrodock/docs/engines",
            );
        });
        ui.add_space(14.0);
        ui.label(RichText::new(reason).size(11.5).color(p.muted))
            .on_hover_text("The error Docker reported");
    });
    click
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tells_permission_problems_apart() {
        assert_eq!(
            diagnose(
                "Error in the hyper legacy client: client error (Connect): Permission denied (os error 13)"
            ),
            Problem::PermissionDenied
        );
        assert_eq!(
            diagnose("Cannot reach the Docker Engine (No such file or directory)"),
            Problem::NotRunning
        );
        assert!(!fixes(Problem::NotRunning).is_empty());
    }
}
