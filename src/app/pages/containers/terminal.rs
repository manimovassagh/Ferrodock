//! Terminal tab: a shell inside the container.

use eframe::egui::{self, RichText};

use crate::app::components::{terminal, widgets};
use crate::app::page::{Intent, PageCtx};
use crate::docker::{ContainerInfo, TARGET_ROOT, TermOp, debug_image, next_session};

/// The shell to run; empty picks bash, then sh.
#[derive(Default)]
pub(in crate::app::pages) struct TerminalTab {
    shell: String,
    /// The size last sent to the shell.
    size: (u16, u16),
    /// The session last shown; a new one takes the keyboard focus.
    shown_session: u64,
}

pub(in crate::app::pages) fn show(
    ui: &mut egui::Ui,
    ctx: &mut PageCtx,
    tab: &mut TerminalTab,
    c: &ContainerInfo,
) {
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
    // Some(debug) when a session should be opened.
    let mut open = term.is_none().then_some(false);

    ui.horizontal(|ui| {
        let label = ui.label(RichText::new("Shell").color(p.muted));
        ui.add(
            egui::TextEdit::singleline(&mut tab.shell)
                .hint_text("bash or sh")
                .desired_width(110.0),
        )
        .labelled_by(label.id);
        if ui
            .button("New session")
            .on_hover_text("Close this shell and start another")
            .clicked()
        {
            open = Some(term.is_some_and(|t| t.debug));
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

    if let Some(debug) = open {
        // Open at the size the screen had last; the first frame corrects it.
        let (cols, rows) = if tab.size.0 > 0 { tab.size } else { (100, 30) };
        ctx.request(Intent::Terminal(TermOp::Open {
            session: next_session(),
            container: c.id.clone(),
            shell: tab.shell.trim().to_owned(),
            debug,
            cols,
            rows,
        }));
        tab.size = (cols, rows);
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
        if term.no_shell && !term.debug {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if widgets::primary_button(ui, "Open a debug shell").clicked() {
                    let (cols, rows) = tab.size;
                    ctx.request(Intent::Terminal(TermOp::Open {
                        session: next_session(),
                        container: c.id.clone(),
                        shell: String::new(),
                        debug: true,
                        cols: cols.max(20),
                        rows: rows.max(4),
                    }));
                }
                ui.label(
                    RichText::new(format!(
                        "Runs {} next to the container, sharing its processes and network.",
                        debug_image()
                    ))
                    .size(12.0)
                    .color(p.muted),
                );
            });
        }
        ui.add_space(6.0);
    } else if term.debug {
        ui.label(
            RichText::new(format!(
                "Debug shell: {} next to {}, sharing its processes and network. \
                 Its files are in {TARGET_ROOT}.",
                debug_image(),
                c.name
            ))
            .size(12.0)
            .color(p.muted),
        );
        ui.add_space(6.0);
    }
    let focus = tab.shown_session != term.session;
    tab.shown_session = term.session;
    let out = terminal::show(ui, p, term.parser.screen(), focus);
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
