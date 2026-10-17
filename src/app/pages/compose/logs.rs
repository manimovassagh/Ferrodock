//! A project's merged logs (`docker compose logs -f`), coloured by service
//! and filterable.

use eframe::egui::{self, RichText};

use crate::app::components::ansi::ansi_color;
use crate::app::page::{Intent, PageCtx};
use crate::docker::{ComposeOp, ComposeProject, next_job, split_log_line};
use crate::theme::Palette;

/// A colour per service, by its place in the project, so neighbours
/// differ. Red comes last: it reads as an error.
fn service_color(p: &Palette, services: &[&str], service: &str) -> egui::Color32 {
    const ORDER: [u8; 6] = [4, 2, 5, 6, 3, 1];
    let i = services.iter().position(|s| *s == service).unwrap_or(0);
    ansi_color(p, ORDER[i % ORDER.len()])
}

/// Draws the logs, starting to follow them if needed. `only` limits them
/// to one service.
pub(super) fn show(
    ui: &mut egui::Ui,
    ctx: &mut PageCtx,
    project: &ComposeProject,
    only: &mut Option<String>,
) {
    let p = ctx.palette;
    let following = ctx
        .store
        .compose
        .logs
        .as_ref()
        .filter(|j| j.project == project.name);
    if following.is_none() {
        ctx.request(Intent::Compose(ComposeOp::Logs {
            job: next_job(),
            project: project.name.clone(),
        }));
    }

    let services = project.services();
    // Filter: all services, or one.
    ui.horizontal_wrapped(|ui| {
        if ui.selectable_label(only.is_none(), "All").clicked() {
            *only = None;
        }
        for service in &services {
            let picked = only.as_deref() == Some(*service);
            let label = RichText::new(*service).color(service_color(p, &services, service));
            if ui.selectable_label(picked, label).clicked() {
                *only = (!picked).then(|| (*service).to_owned());
            }
        }
    });
    ui.add_space(6.0);

    let Some(job) = following else {
        ui.spinner();
        return;
    };
    if let Some(Some(error)) = &job.ended {
        ui.label(RichText::new(error).color(p.danger));
    }
    egui::ScrollArea::both()
        .id_salt(("compose-logs", &project.name))
        .stick_to_bottom(true)
        .auto_shrink(false)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            let font = egui::FontId::monospace(12.0);
            let width = services.iter().map(|s| s.len()).max().unwrap_or(0);
            for line in &job.lines {
                let (service, text) = split_log_line(line);
                if only.as_deref().is_some_and(|o| o != service) {
                    continue;
                }
                let mut job = egui::text::LayoutJob::default();
                if !service.is_empty() {
                    job.append(
                        &format!("{service:>width$}  "),
                        0.0,
                        egui::TextFormat::simple(
                            font.clone(),
                            service_color(p, &services, service),
                        ),
                    );
                }
                job.append(text, 0.0, egui::TextFormat::simple(font.clone(), p.text));
                ui.add(egui::Label::new(job).extend());
            }
        });
}

#[cfg(test)]
mod tests {
    #[test]
    fn colours_differ_between_services() {
        let p = &crate::theme::LIGHT;
        let services = ["web", "worker"];
        let color = |s| super::service_color(p, &services, s);
        assert_eq!(color("web"), color("web"));
        assert_ne!(color("web"), color("worker"));
    }
}
