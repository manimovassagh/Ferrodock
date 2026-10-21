//! The "CLI & MCP" dialog: how to use Ferrodock from a terminal (`fdock`)
//! and from AI tools (`ferrodock-mcp`), with commands ready to copy. Both
//! programs ship next to Ferrodock itself.
use eframe::egui::{self, CornerRadius, Frame, Margin, RichText, Stroke};

use super::cli_install;
use super::components::widgets;
use crate::theme::Palette;

const GUIDE: &str = "https://manimovassagh.github.io/Ferrodock/docs/cli-and-mcp";

/// The full path of `program`, for config files: the apps that read them
/// don't always see the PATH a terminal has.
fn full_path(program: &str, found: Option<&cli_install::Found>) -> String {
    found
        .and_then(|f| f.path(program).cloned())
        .or_else(|| {
            let file = if cfg!(windows) {
                format!("{program}.exe")
            } else {
                program.to_owned()
            };
            cli_install::bundled_dir().map(|dir| dir.join(file))
        })
        .map_or_else(|| program.to_owned(), |path| path.display().to_string())
}

/// How an AI tool adds an MCP server.
enum Setup {
    /// A command to run in a terminal.
    Command(String),
    /// JSON to add to the tool's config file.
    Config { file: &'static str, json: String },
}

struct Tool {
    name: &'static str,
    /// The setup, for the server started by the command (first), which
    /// is at the full path (second).
    setup: fn(&str, &str) -> Setup,
}

/// The AI tools with a known setup, most used first.
const TOOLS: &[Tool] = &[
    Tool {
        name: "Claude Code",
        setup: |server, _| Setup::Command(format!("claude mcp add ferrodock -- {server}")),
    },
    Tool {
        name: "Codex",
        setup: |server, _| Setup::Command(format!("codex mcp add ferrodock -- {server}")),
    },
    Tool {
        name: "GitHub Copilot",
        setup: |server, _| {
            let entry = serde_json::json!({ "name": "ferrodock", "command": unquote(server) });
            Setup::Command(format!("code --add-mcp '{entry}'"))
        },
    },
    Tool {
        name: "Gemini CLI",
        setup: |server, _| Setup::Command(format!("gemini mcp add ferrodock {server}")),
    },
    Tool {
        name: "Cursor",
        setup: |_, path| Setup::Config {
            file: "~/.cursor/mcp.json",
            json: mcp_config(path),
        },
    },
    Tool {
        name: "Windsurf",
        setup: |_, path| Setup::Config {
            file: "~/.codeium/windsurf/mcp_config.json",
            json: mcp_config(path),
        },
    },
    Tool {
        name: "Claude Desktop",
        setup: |_, path| Setup::Config {
            file: if cfg!(windows) {
                r"%APPDATA%\Claude\claude_desktop_config.json"
            } else if cfg!(target_os = "macos") {
                "~/Library/Application Support/Claude/claude_desktop_config.json"
            } else {
                "~/.config/Claude/claude_desktop_config.json"
            },
            json: mcp_config(path),
        },
    },
];

/// A path without the quotes a shell would need around it.
fn unquote(command: &str) -> &str {
    command.trim_matches('"')
}

/// The JSON most AI tools take to add an MCP server.
fn mcp_config(command: &str) -> String {
    let config = serde_json::json!({
        "mcpServers": { "ferrodock": { "command": unquote(command) } }
    });
    serde_json::to_string_pretty(&config).unwrap_or_default()
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

/// Whether `fdock` is on the PATH, with a button to put it there and the
/// steps to do it by hand.
fn cli_status(ui: &mut egui::Ui, p: &Palette, found: Option<&cli_install::Found>) {
    // Still looking: say nothing rather than something that may be wrong.
    let Some(found) = found else {
        ui.spinner();
        return;
    };
    let Some(dir) = cli_install::bundled_dir() else {
        if !found.installed() {
            ui.label(
                RichText::new(
                    "fdock isn't next to this copy of Ferrodock. It comes with every \
                     download, or install it with: cargo install ferrodock-cli ferrodock-mcp",
                )
                .size(12.0)
                .color(p.warning),
            );
        }
        return;
    };
    let progress = cli_install::progress();
    if found.installed() {
        ui.horizontal(|ui| {
            widgets::dot(ui, p.success);
            ui.label(
                RichText::new("Installed. fdock works in any new terminal.")
                    .size(12.5)
                    .color(p.text),
            );
        });
        return;
    }
    ui.horizontal(|ui| {
        let running = progress == cli_install::Progress::Running;
        let clicked = ui
            .add_enabled_ui(!running, |ui| {
                widgets::primary_button(ui, "Install fdock command")
            })
            .inner
            .clicked();
        if clicked {
            let ctx = ui.ctx().clone();
            cli_install::start(dir.clone(), move || ctx.request_repaint());
        }
        if running {
            ui.spinner();
        }
        let note = if cli_install::needs_password() {
            "Your password is needed once."
        } else {
            "Puts fdock and ferrodock-mcp on your PATH."
        };
        ui.label(RichText::new(note).size(12.0).color(p.muted));
    });
    if let cli_install::Progress::Failed(error) = &progress {
        ui.label(
            RichText::new(format!("Couldn't install: {error}"))
                .size(12.0)
                .color(p.danger),
        );
    }
    egui::CollapsingHeader::new(RichText::new("Or do it yourself").size(12.0).color(p.muted))
        .id_salt("cli-manual")
        .show(ui, |ui| {
            let hint = if cfg!(windows) {
                "In PowerShell, then open a new terminal:"
            } else {
                "In a terminal, then open a new one:"
            };
            ui.label(RichText::new(hint).size(12.0).color(p.muted));
            for step in cli_install::manual_steps(&dir) {
                command_row(ui, p, &step);
                ui.add_space(2.0);
            }
        });
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

            widgets::dialog_body(ui, |ui| {
                heading(ui, p, "COMMAND LINE: FDOCK");
                ui.label(
                    RichText::new(
                        "Containers, logs, Quick start and more from a terminal. Add --json for \
                     scripts.",
                    )
                    .color(p.text),
                );
                ui.add_space(8.0);
                let ctx = ui.ctx().clone();
                let found = cli_install::found(move || ctx.request_repaint());
                cli_status(ui, p, found.as_ref());
                ui.add_space(8.0);
                command_row(ui, p, "fdock ps");
                ui.add_space(4.0);
                command_row(ui, p, "fdock quickstart postgres");

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
                let picked_id = egui::Id::unique("ferrodock-integrations-tool");
                let mut picked = ui
                    .ctx()
                    .data(|d| d.get_temp::<usize>(picked_id))
                    .unwrap_or(0)
                    .min(TOOLS.len() - 1);
                ui.horizontal_wrapped(|ui| {
                    for (i, tool) in TOOLS.iter().enumerate() {
                        if ui.selectable_label(i == picked, tool.name).clicked() {
                            picked = i;
                        }
                    }
                });
                ui.ctx().data_mut(|d| d.insert_temp(picked_id, picked));
                ui.add_space(6.0);
                let path = full_path("ferrodock-mcp", found.as_ref());
                match (TOOLS[picked].setup)("ferrodock-mcp", &path) {
                    Setup::Command(command) => command_row(ui, p, &command),
                    Setup::Config { file, json } => {
                        ui.label(
                            RichText::new(format!("Add this to {file}:"))
                                .size(12.0)
                                .color(p.muted),
                        );
                        ui.add_space(2.0);
                        command_row(ui, p, &json);
                    }
                }
            });

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

    #[test]
    fn every_tool_has_a_setup() {
        for tool in TOOLS {
            let server = "\"C:\\Program Files\\ferrodock-mcp.exe\"";
            match (tool.setup)(server, server) {
                Setup::Command(command) => {
                    assert!(command.contains("ferrodock"), "{}", tool.name);
                    if let Some(json) = command.split('\'').nth(1) {
                        let entry: serde_json::Value = serde_json::from_str(json).unwrap();
                        assert_eq!(entry["command"], "C:\\Program Files\\ferrodock-mcp.exe");
                    }
                }
                Setup::Config { file, json } => {
                    assert!(file.ends_with(".json"), "{}", tool.name);
                    serde_json::from_str::<serde_json::Value>(&json).unwrap();
                }
            }
        }
    }
}
