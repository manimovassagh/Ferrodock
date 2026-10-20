//! The "CLI & MCP" dialog: how to use Ferrodock from a terminal (`fdock`)
//! and from AI tools (`ferrodock-mcp`), with commands ready to copy. Both
//! programs ship next to Ferrodock itself.

use std::path::PathBuf;

use eframe::egui::{self, CornerRadius, Frame, Margin, RichText, Stroke};

use super::components::widgets;
use crate::theme::Palette;

const GUIDE: &str = "https://manimovassagh.github.io/Ferrodock/docs/cli-and-mcp";

/// How to run one of the programs that came with Ferrodock: by name when
/// it's on PATH, else by its full path (quoted when it has spaces). None
/// when it isn't next to Ferrodock (e.g. a build from source).
fn invocation(name: &str) -> Option<String> {
    let file = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_owned()
    };
    let beside: PathBuf = std::env::current_exe().ok()?.parent()?.join(&file);
    let on_path = std::env::var_os("PATH")
        .is_some_and(|path| std::env::split_paths(&path).any(|dir| dir.join(&file).is_file()));
    if on_path {
        return Some(name.to_owned());
    }
    if !beside.is_file() {
        return None;
    }
    let full = beside.display().to_string();
    Some(if full.contains(' ') {
        format!("\"{full}\"")
    } else {
        full
    })
}

/// The JSON most AI tools take to add an MCP server.
fn mcp_config(command: &str) -> String {
    let command = command.trim_matches('"').replace('\\', "\\\\");
    format!(
        "{{\n  \"mcpServers\": {{\n    \"ferrodock\": {{ \"command\": \"{command}\" }}\n  }}\n}}"
    )
}

/// One command, in a code box with a Copy button.
fn command_row(ui: &mut egui::Ui, p: &Palette, command: &str) {
    Frame::new()
        .fill(p.bg)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(CornerRadius::same(7))
        .inner_margin(Margin::symmetric(10, 6))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(command).monospace().color(p.text));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("Copy").clicked() {
                        ui.ctx().copy_text(command.to_owned());
                    }
                });
            });
        });
}

fn heading(ui: &mut egui::Ui, p: &Palette, text: &str) {
    ui.label(RichText::new(text).size(11.0).color(p.muted));
    ui.add_space(4.0);
}

/// Draws the dialog. Returns false once it should close.
pub fn show(ui: &mut egui::Ui, p: &Palette) -> bool {
    let mut close = false;
    let modal = egui::Modal::new(egui::Id::unique("ferrodock-integrations"))
        .frame(
            Frame::new()
                .fill(p.sidebar)
                .stroke(Stroke::new(1.0, p.border))
                .corner_radius(CornerRadius::same(10))
                .inner_margin(Margin::same(20)),
        )
        .show(ui.ctx(), |ui| {
            ui.set_width(480.0);
            ui.label(
                RichText::new("Ferrodock in your terminal and AI tools")
                    .size(18.0)
                    .strong()
                    .color(p.text),
            );
            ui.add_space(4.0);
            ui.label(
                RichText::new(
                    "Two programs come with Ferrodock, using the same engine as the app.",
                )
                .color(p.muted),
            );
            ui.add_space(14.0);

            let fdock = invocation("fdock");
            let mcp = invocation("ferrodock-mcp");

            heading(ui, p, "COMMAND LINE: FDOCK");
            ui.label(
                RichText::new(
                    "Containers, logs, Quick start and more from a terminal. Add --json for \
                     scripts.",
                )
                .color(p.text),
            );
            ui.add_space(6.0);
            let run = fdock.as_deref().unwrap_or("fdock");
            for args in ["ps", "quickstart postgres", "logs api -f"] {
                command_row(ui, p, &format!("{run} {args}"));
                ui.add_space(4.0);
            }

            ui.add_space(12.0);
            heading(ui, p, "AI TOOLS: FERRODOCK-MCP");
            ui.label(
                RichText::new(
                    "An MCP server, so AI coding tools can see your containers and start, stop \
                     or run them for you. Add --read-only to let them look without changing \
                     anything.",
                )
                .color(p.text),
            );
            ui.add_space(6.0);
            let server = mcp.as_deref().unwrap_or("ferrodock-mcp");
            command_row(ui, p, &format!("claude mcp add ferrodock -- {server}"));
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if ui.button("Copy JSON config").clicked() {
                    ui.ctx().copy_text(mcp_config(server));
                }
                ui.label(
                    RichText::new("for Cursor, VS Code and other MCP clients")
                        .size(12.0)
                        .color(p.muted),
                );
            });

            if fdock.is_none() || mcp.is_none() {
                ui.add_space(10.0);
                ui.label(
                    RichText::new(
                        "These programs aren't next to this copy of Ferrodock. They come with \
                         every download, or install them with \
                         cargo install ferrodock-cli ferrodock-mcp.",
                    )
                    .size(12.0)
                    .color(p.warning),
                );
            }

            ui.add_space(16.0);
            ui.horizontal(|ui| {
                ui.hyperlink_to("Read the guide", GUIDE);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    close = widgets::primary_button(ui, "Done").clicked();
                });
            });
        });
    !(close || modal.should_close())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_a_json_config() {
        let json = mcp_config("\"C:\\Program Files\\Ferrodock\\ferrodock-mcp.exe\"");
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(
            value["mcpServers"]["ferrodock"]["command"],
            "C:\\Program Files\\Ferrodock\\ferrodock-mcp.exe"
        );
        let value: serde_json::Value = serde_json::from_str(&mcp_config("ferrodock-mcp")).unwrap();
        assert_eq!(value["mcpServers"]["ferrodock"]["command"], "ferrodock-mcp");
    }
}
