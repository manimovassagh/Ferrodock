//! Docker Hub search results, shown under the Pull box.

use eframe::egui::{self, CornerRadius, RichText, Stroke};

use crate::app::components::{icons::Icon, widgets};
use crate::app::page::{Intent, PageCtx};
use crate::theme::Palette;

/// What was clicked on a result.
enum Click {
    Pull,
    Run,
}

/// Draws the results of the last search. Returns false once closed; puts
/// the image to run in `run` when Run is clicked.
pub(super) fn show(ui: &mut egui::Ui, ctx: &mut PageCtx, run: &mut Option<String>) -> bool {
    let p = ctx.palette;
    // The search asked for this frame reaches the store after it.
    let Some((term, results)) = &ctx.store.search else {
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
                ui.label(
                    RichText::new(format!("Docker Hub: “{term}”"))
                        .strong()
                        .color(p.text),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if widgets::icon_button(ui, Icon::Close, "Close the results").clicked() {
                        open = false;
                    }
                    // A Quick start template for this, with its defaults.
                    if let Some(template) = crate::docker::TEMPLATES
                        .iter()
                        .find(|t| t.is_image(term) || t.name.eq_ignore_ascii_case(term))
                        && ui
                            .link(format!("{} quick start, with defaults →", template.name))
                            .clicked()
                    {
                        ctx.request(Intent::Open {
                            page: "Quick start",
                            id: template.name.to_owned(),
                        });
                    }
                });
            });
            ui.add_space(4.0);
            match results {
                None => {
                    ui.spinner();
                }
                Some(Err(e)) => {
                    ui.label(RichText::new(format!("Couldn't search: {e}")).color(p.danger));
                }
                Some(Ok(list)) if list.is_empty() => {
                    ui.label(RichText::new("Nothing found.").color(p.muted));
                }
                Some(Ok(list)) => {
                    egui::ScrollArea::vertical()
                        .max_height(260.0)
                        .show(ui, |ui| {
                            for result in list {
                                match row(ui, p, result) {
                                    Some(Click::Pull) => {
                                        ctx.request(Intent::Pull(result.name.clone()));
                                    }
                                    Some(Click::Run) => *run = Some(result.name.clone()),
                                    None => {}
                                }
                            }
                        });
                }
            }
        });
    ui.add_space(8.0);
    open
}

/// One result, with Pull and Run.
fn row(ui: &mut egui::Ui, p: &Palette, result: &crate::docker::SearchResult) -> Option<Click> {
    let mut click = None;
    ui.horizontal(|ui| {
        // Leave room for the buttons.
        let text_width = (ui.available_width() - 190.0).max(120.0);
        ui.vertical(|ui| {
            ui.set_max_width(text_width);
            ui.horizontal(|ui| {
                ui.label(RichText::new(&result.name).strong().color(p.text));
                if result.official {
                    widgets::pill(ui, "Official", p.success);
                }
                ui.label(
                    RichText::new(format!("★ {}", thousands(result.stars)))
                        .size(12.0)
                        .color(p.muted),
                );
            });
            if !result.description.is_empty() {
                ui.add(
                    egui::Label::new(RichText::new(&result.description).size(12.0).color(p.muted))
                        .truncate(),
                );
            }
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if widgets::button(ui, Icon::Play, "Run…")
                .on_hover_text(format!("Run {} as a container", result.name))
                .clicked()
            {
                click = Some(Click::Run);
            }
            if widgets::button(ui, Icon::Download, "Pull")
                .on_hover_text(format!("Pull {}", result.name))
                .clicked()
            {
                click = Some(Click::Pull);
            }
        });
    });
    ui.add_space(6.0);
    click
}

/// 21408 → "21,408".
fn thousands(n: i64) -> String {
    let digits = n.abs().to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    if n < 0 { format!("-{out}") } else { out }
}

#[cfg(test)]
mod tests {
    #[test]
    fn groups_thousands() {
        assert_eq!(super::thousands(21408), "21,408");
        assert_eq!(super::thousands(124), "124");
        assert_eq!(super::thousands(1_000_000), "1,000,000");
    }
}
