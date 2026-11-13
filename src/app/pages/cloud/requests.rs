//! The request log: every request the emulator answered, newest last,
//! read while the tab is open. A row opens what was sent and what came
//! back.

use std::time::{Duration, Instant};

use eframe::egui::{self, RichText};

use super::pretty;
use crate::app::components::{details, widgets};
use crate::app::page::{Intent, PageCtx};
use crate::app::store::Store;
use crate::docker::cloud::{CloudBody, CloudOp, CloudRequest};
use crate::docker::format_size;
use crate::theme::Palette;

/// How often the emulator is asked for new requests while the log shows.
const POLL: Duration = Duration::from_secs(1);

/// The log's own state: what's filtered, what's open.
#[derive(Default)]
pub struct RequestLog {
    /// Only this service's requests; empty for all.
    service: String,
    errors_only: bool,
    /// The request open in the details pane.
    pub selected: Option<u64>,
    /// When the emulator was last asked for new requests.
    polled: Option<Instant>,
}

/// `14:03:07`, UTC, from milliseconds since 1970.
fn clock(ms: u64) -> String {
    let secs = ms / 1000 % 86_400;
    format!("{:02}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60)
}

fn status_color(p: &Palette, request: &CloudRequest) -> egui::Color32 {
    match request.status {
        0..=399 => p.success,
        400..=499 => p.warning,
        _ => p.danger,
    }
}

impl RequestLog {
    fn shows(&self, ctx: &PageCtx, r: &CloudRequest) -> bool {
        (self.service.is_empty() || r.service == self.service)
            && (!self.errors_only || r.failed())
            && ctx.matches_search(&[
                &r.service,
                &r.operation,
                &r.resource,
                &r.error,
                &r.path,
                &r.user_agent,
            ])
    }

    pub fn show(&mut self, ui: &mut egui::Ui, ctx: &mut PageCtx, port: u16) {
        let p = ctx.palette;
        let cloud = &ctx.store.cloud;
        // Read only while this shows: nothing is asked with the log shut.
        if !cloud.polling && self.polled.is_none_or(|at| at.elapsed() >= POLL) {
            self.polled = Some(Instant::now());
            ctx.request(Intent::Cloud(CloudOp::Requests {
                port,
                after: cloud.last_request(),
            }));
        }
        ui.ctx().request_repaint_after(POLL);

        let mut services: Vec<&str> = cloud.requests.iter().map(|r| r.service.as_str()).collect();
        services.sort_unstable();
        services.dedup();
        ui.horizontal(|ui| {
            let all = "All services";
            egui::ComboBox::from_id_salt("cloud-request-service")
                .selected_text(if self.service.is_empty() {
                    all
                } else {
                    &self.service
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.service, String::new(), all);
                    for service in services.iter().filter(|s| !s.is_empty()) {
                        ui.selectable_value(&mut self.service, (*service).to_owned(), *service);
                    }
                });
            ui.checkbox(&mut self.errors_only, "Errors only");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let shown = cloud.requests.iter().filter(|r| self.shows(ctx, r)).count();
                let count = if shown == cloud.requests.len() {
                    crate::docker::plural(shown, "request")
                } else {
                    format!(
                        "{shown} of {}",
                        crate::docker::plural(cloud.requests.len(), "request")
                    )
                };
                ui.label(RichText::new(count).size(12.5).color(p.muted));
            });
        });
        ui.add_space(8.0);

        if let Some(error) = &cloud.requests_error {
            ui.label(RichText::new(error).color(p.danger));
            return;
        }
        if cloud.requests.is_empty() {
            ui.add_space(40.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("No requests yet").strong().color(p.text));
                ui.add_space(4.0);
                ui.label(
                    RichText::new(
                        "Point a tool at the emulator and each request shows here as it's answered.",
                    )
                    .color(p.muted),
                );
            });
            return;
        }

        let rows: Vec<&CloudRequest> = cloud
            .requests
            .iter()
            .filter(|r| self.shows(ctx, r))
            .collect();
        if rows.is_empty() {
            ui.label(RichText::new("No request matches.").color(p.muted));
            return;
        }
        let mut clicked = None;
        egui::ScrollArea::vertical()
            .id_salt("cloud-requests")
            .auto_shrink(false)
            .stick_to_bottom(true)
            .show_rows(ui, 26.0, rows.len(), |ui, range| {
                for r in &rows[range] {
                    if self.row(ui, p, r) {
                        clicked = Some(r.id);
                    }
                }
            });
        if let Some(id) = clicked {
            self.selected = if self.selected == Some(id) {
                None
            } else {
                Some(id)
            };
        }
    }

    /// One request on a line. True when it's clicked.
    fn row(&self, ui: &mut egui::Ui, p: &Palette, r: &CloudRequest) -> bool {
        let selected = self.selected == Some(r.id);
        let (rect, response) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), 26.0), egui::Sense::click());
        if selected {
            ui.painter().rect_filled(
                rect,
                egui::CornerRadius::same(6),
                p.accent.gamma_multiply(0.12),
            );
        } else if response.hovered() {
            ui.painter().rect_filled(
                rect,
                egui::CornerRadius::same(6),
                p.border.gamma_multiply(0.5),
            );
        }
        let mut row = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(rect.shrink2(egui::vec2(8.0, 0.0)))
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        let ui = &mut row;
        let cell = |ui: &mut egui::Ui, width: f32, text: RichText| {
            ui.allocate_ui_with_layout(
                egui::vec2(width, 26.0),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.set_width(width);
                    ui.add(egui::Label::new(text).truncate().selectable(false));
                },
            );
        };
        cell(
            ui,
            62.0,
            RichText::new(clock(r.time))
                .monospace()
                .size(12.0)
                .color(p.muted),
        );
        cell(
            ui,
            34.0,
            RichText::new(r.status.to_string())
                .monospace()
                .size(12.0)
                .strong()
                .color(status_color(p, r)),
        );
        cell(
            ui,
            70.0,
            RichText::new(&r.service).size(12.5).color(p.muted),
        );
        let operation = if r.operation.is_empty() {
            format!("{} {}", r.method, r.path)
        } else {
            r.operation.clone()
        };
        let wide = rect.width() >= 820.0;
        cell(
            ui,
            if wide { 230.0 } else { 190.0 },
            RichText::new(operation).size(13.0).color(p.text),
        );
        // What's left goes to what it was about (or why it failed). Who
        // sent it shows only where there's room, as with the pane shut.
        let client = if wide { 150.0 } else { 0.0 };
        let spacing = ui.spacing().item_spacing.x * 3.0;
        let rest = (ui.available_width() - 62.0 - client - spacing).max(40.0);
        let about = if r.failed() && !r.error.is_empty() {
            RichText::new(&r.error).size(12.5).color(status_color(p, r))
        } else {
            RichText::new(&r.resource)
                .monospace()
                .size(12.0)
                .color(p.muted)
        };
        cell(ui, rest, about);
        if wide {
            cell(
                ui,
                client,
                RichText::new(r.client()).size(12.0).color(p.muted),
            );
        }
        cell(
            ui,
            62.0,
            RichText::new(format!("{} ms", r.duration_ms))
                .size(12.0)
                .color(p.muted),
        );
        response
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .clicked()
    }

    /// The selected request, if the log still has it.
    fn selected<'a>(&self, store: &'a Store) -> Option<&'a CloudRequest> {
        let id = self.selected?;
        store.cloud.requests.iter().find(|r| r.id == id)
    }

    /// The details pane: the request's line in full, then what was sent
    /// and what came back.
    pub fn show_details(&mut self, ui: &mut egui::Ui, ctx: &mut PageCtx, port: u16) {
        let p = ctx.palette;
        let store = ctx.store;
        let Some(r) = self.selected(store) else {
            return;
        };
        let title = if r.operation.is_empty() {
            format!("{} {}", r.method, r.path)
        } else {
            r.operation.clone()
        };
        if details::header(ui, p, &title) {
            self.selected = None;
            return;
        }
        widgets::pill(
            ui,
            &if r.failed() {
                format!("{} {}", r.status, r.error)
            } else {
                r.status.to_string()
            },
            status_color(p, r),
        );
        if r.failed() && !r.message.is_empty() {
            ui.add_space(6.0);
            ui.label(RichText::new(&r.message).color(p.text));
        }
        ui.add_space(10.0);
        ctx.request(Intent::Cloud(CloudOp::Detail { port, id: r.id }));
        let detail = match &store.cloud.detail {
            Some((id, detail)) if *id == r.id => detail.as_ref(),
            _ => None,
        };

        egui::ScrollArea::vertical()
            .id_salt("cloud-request-details")
            .auto_shrink(false)
            .show(ui, |ui| {
                details::section(ui, p, "Request", |ui| {
                    details::field(ui, p, "Service", &r.service);
                    details::field(ui, p, "Resource", &r.resource);
                    details::field(ui, p, "Sent", &format!("{} {}", r.method, r.path));
                    details::field(ui, p, "Time", &format!("{} UTC", clock(r.time)));
                    details::field(ui, p, "Took", &format!("{} ms", r.duration_ms));
                    details::field(ui, p, "Region", &r.region);
                    details::field(ui, p, "From", &r.user_agent);
                    details::copy_field(ui, p, "Request ID", &r.request_id, &r.request_id);
                });
                match detail {
                    None => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(RichText::new("Reading what was sent…").color(p.muted));
                        });
                    }
                    Some(Err(error)) => {
                        ui.label(RichText::new(error).color(p.muted));
                    }
                    Some(Ok(detail)) => {
                        body(ui, p, "Sent", &detail.request_body);
                        body(ui, p, "Answered", &detail.response_body);
                        headers(ui, p, "Request headers", &detail.request_headers);
                        headers(ui, p, "Response headers", &detail.response_headers);
                    }
                }
            });
    }
}

/// A body in a box, readable, with a note on what isn't shown.
fn body(ui: &mut egui::Ui, p: &Palette, title: &str, body: &CloudBody) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(title.to_uppercase())
                .size(11.0)
                .color(p.muted),
        );
        if body.size > 0 {
            ui.label(
                RichText::new(format_size(body.size as i64))
                    .size(11.0)
                    .color(p.muted),
            );
        }
        if !body.text.is_empty() {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("Copy").clicked() {
                    ui.ctx().copy_text(pretty::body(&body.text));
                }
            });
        }
    });
    ui.add_space(2.0);
    egui::Frame::new()
        .fill(p.bg)
        .stroke(egui::Stroke::new(1.0, p.border))
        .corner_radius(egui::CornerRadius::same(8))
        .inner_margin(egui::Margin::same(10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let note = if body.streamed {
                Some(format!(
                    "Streamed: {} went by without being kept",
                    format_size(body.size as i64)
                ))
            } else if body.binary {
                Some(format!(
                    "Not text: {} of data",
                    format_size(body.size as i64)
                ))
            } else if body.size == 0 {
                Some("Empty".to_owned())
            } else {
                None
            };
            match note {
                Some(note) => {
                    ui.label(RichText::new(note).color(p.muted));
                }
                None => {
                    ui.add(
                        egui::Label::new(
                            RichText::new(pretty::body(&body.text))
                                .monospace()
                                .size(12.0)
                                .color(p.text),
                        )
                        .wrap(),
                    );
                    if body.truncated {
                        ui.add_space(4.0);
                        ui.label(
                            RichText::new(format!(
                                "Only the start is kept: {} in all",
                                format_size(body.size as i64)
                            ))
                            .size(12.0)
                            .color(p.muted),
                        );
                    }
                }
            }
        });
    ui.add_space(14.0);
}

fn headers(ui: &mut egui::Ui, p: &Palette, title: &str, headers: &[(String, String)]) {
    if headers.is_empty() {
        return;
    }
    details::section(ui, p, title, |ui| {
        for (name, value) in headers {
            details::field(ui, p, name, value);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tells_the_time_of_day() {
        assert_eq!(clock(0), "00:00:00");
        assert_eq!(clock(1_794_528_187_000), "00:03:07");
        assert_eq!(clock(1_794_528_187_000 + 13 * 3_600_000), "13:03:07");
    }
}
