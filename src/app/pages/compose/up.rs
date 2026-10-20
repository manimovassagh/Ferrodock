//! Bringing a project up from its Compose file: a path box, files dropped
//! on the window, recent files, and the output of the last command.

use std::path::{Path, PathBuf};

use eframe::egui::{self, CornerRadius, RichText, Stroke};

use crate::app::components::{icons::Icon, widgets};
use crate::app::page::{Intent, PageCtx};
use crate::docker::{ComposeAction, ComposeOp, ComposeTarget, next_job};

/// Whether `path` looks like a Compose file.
pub(super) fn is_compose_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("yaml") || e.eq_ignore_ascii_case("yml"))
}

/// `docker compose up` for `file`.
pub fn up_op(file: &Path) -> ComposeOp {
    ComposeOp::Run {
        job: next_job(),
        target: ComposeTarget::from_file(file),
        action: ComposeAction::Up,
    }
}

/// Starts `docker compose up` for `file`.
pub(super) fn up(ctx: &mut PageCtx, file: &Path) {
    ctx.request(Intent::Compose(up_op(file)));
}

/// Asks for a Compose file with the system's file picker.
pub fn pick_compose_file() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Open a Compose file")
        .add_filter("Compose file", &["yaml", "yml"])
        .pick_file()
}

/// The path box, the recent files and the drop target.
pub(super) fn bar(ui: &mut egui::Ui, ctx: &mut PageCtx, path: &mut String) {
    let p = ctx.palette;
    let (hovering, dropped) = ui.input(|i| {
        let dropped: Vec<PathBuf> = i
            .raw
            .dropped_files
            .iter()
            .map(|f| f.path().to_path_buf())
            .filter(|f| is_compose_file(f))
            .collect();
        (!i.raw.hovered_files.is_empty(), dropped)
    });
    for file in dropped {
        up(ctx, &file);
    }
    if hovering {
        egui::Frame::new()
            .fill(p.accent.gamma_multiply(0.15))
            .stroke(Stroke::new(1.5, p.accent))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(egui::Margin::same(12))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(
                    RichText::new("Drop a compose.yaml to start its project")
                        .strong()
                        .color(p.accent),
                );
            });
        ui.add_space(8.0);
    }

    ui.horizontal(|ui| {
        let field = ui.add(
            egui::TextEdit::singleline(path)
                .hint_text("Path to a compose.yaml, or drop one here")
                .desired_width(340.0),
        );
        let file = PathBuf::from(path.trim());
        let ok = file.is_file();
        let start = ui
            .add_enabled_ui(ok, |ui| widgets::button(ui, Icon::Play, "Up"))
            .inner
            .on_hover_text("docker compose up -d")
            .on_disabled_hover_text("Enter the path of a Compose file on this computer");
        let entered = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if widgets::button(ui, Icon::Folder, "Open…")
            .on_hover_text("Pick a Compose file and run docker compose up -d")
            .clicked()
            && let Some(file) = pick_compose_file()
        {
            up(ctx, &file);
        }
        // The button names the field for screen readers.
        field.labelled_by(start.id);
        if ok && (start.clicked() || entered) {
            up(ctx, &file);
            path.clear();
        }
    });
    let recent: Vec<String> = ctx
        .store
        .recent_compose
        .iter()
        .filter(|f| Path::new(f).is_file())
        .cloned()
        .collect();
    if !recent.is_empty() {
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Recent").size(12.0).color(p.muted));
            for file in recent {
                let path = Path::new(&file);
                let name = ComposeTarget::from_file(path).label();
                if widgets::button(ui, Icon::Play, &name)
                    .on_hover_text(format!("docker compose up -d\n{file}"))
                    .clicked()
                {
                    up(ctx, path);
                }
            }
        });
    }
    ui.add_space(8.0);
}

/// The output of the last command, until closed. Returns false once the
/// user closes it.
pub(super) fn output(ui: &mut egui::Ui, ctx: &PageCtx) -> bool {
    let p = ctx.palette;
    let Some(job) = &ctx.store.compose.run else {
        return true;
    };
    let mut open = true;
    egui::Frame::new()
        .fill(p.bg)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(egui::Margin::same(10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                match &job.ended {
                    None => {
                        ui.spinner();
                    }
                    Some(None) => widgets::dot(ui, p.success),
                    Some(Some(_)) => widgets::dot(ui, p.danger),
                }
                ui.label(RichText::new(&job.title).strong().color(p.text));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if job.ended.is_some()
                        && widgets::icon_button(ui, Icon::Close, "Close the output").clicked()
                    {
                        open = false;
                    }
                });
            });
            if let Some(Some(error)) = &job.ended {
                ui.label(RichText::new(error).color(p.danger));
            }
            egui::ScrollArea::vertical()
                .id_salt(("compose-output", job.id))
                .max_height(180.0)
                .stick_to_bottom(true)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    for line in &job.lines {
                        ui.label(RichText::new(line).monospace().size(12.0).color(p.muted));
                    }
                });
        });
    ui.add_space(8.0);
    open
}
