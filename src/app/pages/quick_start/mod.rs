//! Quick start page: popular services (Postgres, Redis, …) as cards, each
//! run in a step with settings that work for local development.

mod form;

use eframe::egui::{self, CornerRadius, RichText, Stroke};

use crate::app::components::icons::Icon;
use crate::app::components::widgets;
use crate::app::page::{Intent, Page, PageCtx};
use crate::app::store::Store;
use crate::docker::{Choices, ContainerInfo, Resource, TEMPLATES, Template};
use crate::theme::Palette;
use form::TemplateForm;

/// Narrowest a card gets before the grid drops a column.
const CARD_WIDTH: f32 = 220.0;

#[derive(Default)]
pub struct QuickStartPage {
    /// The Run dialog, while it's open.
    form: Option<TemplateForm>,
}

/// The running container made from `template`, if any.
pub fn running<'a>(store: &'a Store, template: &Template) -> Option<&'a ContainerInfo> {
    store
        .containers
        .iter()
        .find(|c| c.is_active() && template.is_image(&c.image))
}

/// Whether a host port is free: no container publishes it and nothing on
/// this machine listens on it.
fn port_free(store: &Store, port: u16) -> bool {
    crate::docker::port_free(&store.containers, port)
}

/// Settings for a run of `template` that don't clash with what's there.
pub fn defaults(store: &Store, template: &Template) -> Choices {
    template.defaults_for(&store.containers)
}

impl Page for QuickStartPage {
    fn title(&self) -> &'static str {
        "Quick start"
    }

    fn icon(&self) -> Icon {
        Icon::Play
    }

    fn resource(&self) -> Resource {
        Resource::Containers
    }

    fn badge(&self, store: &Store) -> String {
        let running = TEMPLATES
            .iter()
            .filter(|t| running(store, t).is_some())
            .count();
        if running == 0 {
            String::new()
        } else {
            running.to_string()
        }
    }

    fn summary(&self, _store: &Store) -> String {
        "Popular services, ready for local development".into()
    }

    fn search_hint(&self) -> &'static str {
        "Filter templates"
    }

    fn bottom(&self) -> bool {
        true
    }

    fn has_overlay(&self) -> bool {
        self.form.is_some()
    }

    fn show(&mut self, ui: &mut egui::Ui, ctx: &mut PageCtx) {
        if let Some(open) = &mut self.form {
            match form::show(ui, ctx, open) {
                Some(form::Outcome::Run(spec)) => {
                    ctx.request(Intent::RunImage(*spec));
                    self.form = None;
                }
                Some(form::Outcome::Cancel) => self.form = None,
                None => {}
            }
        }
        let p = ctx.palette;
        let visible: Vec<&'static Template> = TEMPLATES
            .iter()
            .filter(|t| ctx.matches_search(&[t.name, t.summary, t.image]))
            .collect();
        if visible.is_empty() {
            if widgets::empty_state(
                ui,
                p,
                Icon::Search,
                "No matches",
                "No template matches your filter.",
                Some("Clear filter"),
            ) {
                ctx.request(Intent::ClearSearch);
            }
            return;
        }
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                let gap = 12.0;
                let width = ui.available_width();
                let columns = (((width + gap) / (CARD_WIDTH + gap)).floor() as usize).max(1);
                let card_width = (width - gap * (columns - 1) as f32) / columns as f32;
                for row in visible.chunks(columns) {
                    ui.horizontal_top(|ui| {
                        ui.spacing_mut().item_spacing.x = gap;
                        for template in row {
                            if let Some(intent) = card(ui, ctx, template, card_width) {
                                match intent {
                                    CardClick::Run => {
                                        self.form = Some(TemplateForm::new(template, ctx.store));
                                    }
                                    CardClick::Show(id) => ctx.request(Intent::Open {
                                        page: "Containers",
                                        id,
                                    }),
                                }
                            }
                        }
                    });
                    ui.add_space(gap);
                }
                ui.add_space(4.0);
                ui.label(
                    RichText::new(
                        "Not here? Search any image on Docker Hub from the Images page and \
                         run it from the results.",
                    )
                    .size(12.5)
                    .color(p.muted),
                );
            });
    }

    fn has_selection(&self, _store: &Store) -> bool {
        false
    }

    fn show_details(&mut self, _ui: &mut egui::Ui, _ctx: &mut PageCtx) {}

    fn clear_selection(&mut self) {}

    /// Opens the Run dialog of the template with this name.
    fn select(&mut self, id: &str) {
        self.form = crate::docker::template(id).map(TemplateForm::without_store);
    }
}

enum CardClick {
    Run,
    /// Open the running container.
    Show(String),
}

/// A template's badge: its initial on its colour.
pub(super) fn badge(ui: &mut egui::Ui, template: &Template, size: f32) {
    let [r, g, b] = template.color;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    ui.painter().rect_filled(
        rect,
        CornerRadius::same((size / 4.0) as u8),
        egui::Color32::from_rgb(r, g, b),
    );
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        &template.name[..1],
        egui::FontId::new(size * 0.45, crate::theme::semibold()),
        egui::Color32::WHITE,
    );
}

fn card(ui: &mut egui::Ui, ctx: &PageCtx, template: &Template, width: f32) -> Option<CardClick> {
    let p: &Palette = ctx.palette;
    let mut click = None;
    // Each card is its own column, laid out top to bottom.
    let layout = egui::Layout::top_down(egui::Align::Min);
    ui.allocate_ui_with_layout(egui::vec2(width, 0.0), layout, |ui| {
        egui::Frame::new()
            .fill(p.bg)
            .stroke(Stroke::new(1.0, p.border))
            .corner_radius(CornerRadius::same(12))
            .inner_margin(egui::Margin::same(14))
            .show(ui, |ui| {
                ui.set_width(width - 30.0);
                ui.horizontal(|ui| {
                    badge(ui, template, 36.0);
                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new(template.name)
                                .font(egui::FontId::new(15.0, crate::theme::semibold()))
                                .color(p.text),
                        );
                        ui.label(RichText::new(template.summary).size(12.5).color(p.muted));
                    });
                    if running(ctx.store, template).is_some() {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                            widgets::pill(ui, "Running", p.success);
                        });
                    }
                });
                ui.add_space(8.0);
                let ports: Vec<String> =
                    template.ports.iter().map(|p| p.host.to_string()).collect();
                ui.add(
                    egui::Label::new(
                        RichText::new(format!(
                            "{}:{} · :{}",
                            template.image,
                            template.tags[0],
                            ports.join(" :")
                        ))
                        .monospace()
                        .size(11.5)
                        .color(p.muted),
                    )
                    .truncate(),
                );
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if widgets::primary_button(ui, "Run…")
                            .on_hover_text(format!("Run {}", template.name))
                            .clicked()
                        {
                            click = Some(CardClick::Run);
                        }
                        if let Some(c) = running(ctx.store, template)
                            && ui
                                .button("Show")
                                .on_hover_text(format!("Open {}", c.name))
                                .clicked()
                        {
                            click = Some(CardClick::Show(c.id.clone()));
                        }
                    });
                });
            });
    });
    click
}
