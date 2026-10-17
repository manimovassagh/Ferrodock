//! Terminal tab: a shell inside the container.

use eframe::egui::{self, RichText};

use crate::app::components::{terminal, widgets};
use crate::app::page::{Intent, PageCtx};
use crate::docker::{ContainerInfo, TermOp, next_session};

/// The shell to run; empty picks bash, then sh.
#[derive(Default)]
pub(super) struct TerminalTab {
    shell: String,
    /// The size last sent to the shell.
    size: (u16, u16),
    /// Focus the terminal on the next frame (after opening it).
    focus: bool,
}

pub(super) fn show(ui: &mut egui::Ui, ctx: &mut PageCtx, tab: &mut TerminalTab, c: &ContainerInfo) {
    let p = ctx.palette;
    if !c.is_running() {
        ui.add_space(20.0);
        ui.label(RichText::new("Start the container to open a shell in it.").color(p.muted));
        return;
    }
    let term = ctx
        .store
        .terminal
        .as_ref()
        .filter(|t| t.container_id == c.id);
    let mut open = term.is_none();

    ui.horizontal(|ui| {
        ui.label(RichText::new("Shell").color(p.muted));
        ui.add(
            egui::TextEdit::singleline(&mut tab.shell)
                .hint_text("bash or sh")
                .desired_width(110.0),
        );
        if ui
            .button("New session")
            .on_hover_text("Close this shell and start another")
            .clicked()
        {
            open = true;
        }
        if let Some(t) = term {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if widgets::icon_button(
                    ui,
                    crate::app::components::icons::Icon::Close,
                    "End the session",
                )
                .clicked()
                {
                    ctx.request(Intent::Terminal(TermOp::Close { session: t.session }));
                }
            });
        }
    });
    ui.add_space(8.0);

    if open {
        // Open at the size the screen had last; the first frame corrects it.
        let (cols, rows) = if tab.size.0 > 0 { tab.size } else { (100, 30) };
        ctx.request(Intent::Terminal(TermOp::Open {
            session: next_session(),
            container: c.id.clone(),
            shell: tab.shell.trim().to_owned(),
            cols,
            rows,
        }));
        tab.size = (cols, rows);
        tab.focus = true;
        return;
    }
    let Some(term) = term else { return };

    if let Some(ended) = &term.ended {
        let (text, color) = match ended {
            Some(why) => (why.as_str(), p.danger),
            None => (
                "The shell exited. Start a new session to open another.",
                p.muted,
            ),
        };
        ui.label(RichText::new(text).color(color));
        ui.add_space(6.0);
    }
    let out = terminal::show(ui, p, term.parser.screen(), std::mem::take(&mut tab.focus));
    if term.ended.is_none() {
        if !out.input.is_empty() {
            ctx.request(Intent::Terminal(TermOp::Input {
                session: term.session,
                bytes: out.input,
            }));
        }
        if out.size != term.parser.screen().size().swap() {
            tab.size = out.size;
            ctx.request(Intent::Terminal(TermOp::Resize {
                session: term.session,
                cols: out.size.0,
                rows: out.size.1,
            }));
        }
    }
}

trait Swap {
    fn swap(self) -> Self;
}

impl Swap for (u16, u16) {
    /// vt100 sizes are (rows, cols); ours are (cols, rows).
    fn swap(self) -> Self {
        (self.1, self.0)
    }
}
