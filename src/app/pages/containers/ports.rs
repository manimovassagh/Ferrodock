//! Published ports as links that open in the browser.

use eframe::egui::{self, RichText};

use crate::app::components::table;
use crate::docker::{PortMapping, engine_host};
use crate::theme::Palette;

/// One port: a link when a browser can open it, plain text otherwise.
/// Right-click copies the address.
fn port(ui: &mut egui::Ui, p: &Palette, port: &PortMapping, text: String) -> egui::Response {
    let response = match port.url(engine_host()) {
        Some(url) => ui
            .add(egui::Hyperlink::from_label_and_url(
                RichText::new(format!("{text} ↗")).color(p.accent),
                &url,
            ))
            .on_hover_text(format!("Open {url}")),
        None => ui.add(egui::Label::new(RichText::new(text).color(p.text)).selectable(false)),
    };
    if let Some(public) = port.public {
        response.context_menu(|ui| {
            if ui.button("Copy address").clicked() {
                ui.ctx().copy_text(format!("{}:{public}", engine_host()));
                ui.close();
            }
        });
    }
    response
}

/// Table cell: the first port, with "+n" for the rest.
pub(super) fn cell(ui: &mut egui::Ui, p: &Palette, ports: &[PortMapping]) {
    let Some(first) = ports.first() else {
        table::muted(ui, p, "—");
        return;
    };
    let text = match first.public {
        Some(public) => public.to_string(),
        None => first.short_label(),
    };
    port(ui, p, first, text);
    if ports.len() > 1 {
        ui.label(
            RichText::new(format!("+{}", ports.len() - 1))
                .size(12.0)
                .color(p.muted),
        )
        .on_hover_text(
            ports
                .iter()
                .map(PortMapping::label)
                .collect::<Vec<_>>()
                .join("\n"),
        );
    }
}

/// Details pane row: every port on its own line.
pub(super) fn field(ui: &mut egui::Ui, p: &Palette, ports: &[PortMapping]) {
    ui.label(RichText::new("Ports").color(p.muted));
    ui.vertical(|ui| {
        if ports.is_empty() {
            ui.label(RichText::new("—").color(p.text));
        }
        for mapping in ports {
            port(ui, p, mapping, mapping.label());
        }
    });
    ui.end_row();
}

/// Every published port as a compact link, e.g. for a Compose service.
pub(in crate::app::pages) fn links(ui: &mut egui::Ui, p: &Palette, ports: &[PortMapping]) {
    for mapping in ports {
        if let Some(public) = mapping.public {
            port(ui, p, mapping, public.to_string());
        }
    }
}
