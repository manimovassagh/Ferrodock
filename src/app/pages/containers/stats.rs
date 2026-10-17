//! Stats tab: live CPU and memory with a one-minute history, plus network
//! and disk totals.

use eframe::egui::{self, RichText};

use crate::app::components::{details, sparkline};
use crate::app::store::Live;
use crate::docker::format_size;
use crate::theme::Palette;

pub(super) fn show(ui: &mut egui::Ui, p: &Palette, live: Option<&Live>, running: bool) {
    let latest = live.and_then(|l| l.stats.back());
    let (Some(live), Some(latest)) = (live, latest) else {
        let message = if running {
            "Waiting for the first reading…"
        } else {
            "Stats are available while the container is running."
        };
        ui.label(RichText::new(message).color(p.muted));
        return;
    };

    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| {
            let cpu: Vec<f64> = live.stats.iter().map(|s| s.cpu_percent).collect();
            let cpu_max = chart_ceiling(&cpu, 1.0);
            metric(ui, p, "CPU", &format!("{:.1}%", latest.cpu_percent));
            sparkline::show(ui, p, &cpu, cpu_max, p.accent);
            ui.add_space(14.0);

            let memory: Vec<f64> = live.stats.iter().map(|s| s.memory_used as f64).collect();
            let memory_max = chart_ceiling(&memory, 1_000_000.0);
            let memory_text = if latest.memory_limit > 0 {
                let percent = latest.memory_used as f64 / latest.memory_limit as f64 * 100.0;
                format!(
                    "{} / {} ({percent:.1}%)",
                    format_size(latest.memory_used as i64),
                    format_size(latest.memory_limit as i64)
                )
            } else {
                format_size(latest.memory_used as i64)
            };
            metric(ui, p, "Memory", &memory_text);
            sparkline::show(ui, p, &memory, memory_max, p.success);
            ui.add_space(14.0);

            details::section(ui, p, "Totals", |ui| {
                let pair = |a: u64, b: u64| {
                    format!("{} / {}", format_size(a as i64), format_size(b as i64))
                };
                details::field(
                    ui,
                    p,
                    "Network in / out",
                    &pair(latest.net_rx, latest.net_tx),
                );
                details::field(
                    ui,
                    p,
                    "Disk read / write",
                    &pair(latest.block_read, latest.block_write),
                );
            });
        });
}

/// Top of a chart: a bit above the highest recent value, so changes stay
/// visible even when usage is tiny compared to the limit.
fn chart_ceiling(values: &[f64], floor: f64) -> f64 {
    values.iter().copied().fold(floor, f64::max) * 1.25
}

fn metric(ui: &mut egui::Ui, p: &Palette, label: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(label.to_uppercase())
                .size(11.0)
                .color(p.muted),
        );
        ui.label(RichText::new(value).strong().color(p.text));
    });
    ui.add_space(2.0);
}
