//! The Ferrodock window.
//!
//! This module only wires things together: it feeds worker responses into
//! the [`Store`], lays out the shell, lets the active [`Page`] draw itself
//! and turns the page's [`Intent`]s into worker requests.

mod autostart;
mod commands;
mod components;
mod engine_start;
mod keys;
mod notify;
mod page;
mod pages;
#[cfg(test)]
mod screenshots;
mod settings;
mod shell;
mod sidebar;
mod store;
mod tray;

use eframe::egui::{self, Frame, Margin};

use crate::docker::{Action, Task};
use crate::theme;
use crate::worker::{Request, Worker};
use commands::Target;
use components::confirm::{self, Choice};
use components::palette::{CommandPalette, Outcome};
use components::toast::Toast;
use components::{shortcuts, table};
use keys::Shortcut;
use page::{CommandRun, Confirmation, Intent, Page, PageCtx};
use store::{Connection, Live, Store};

pub struct FerrodockApp {
    worker: Worker,
    store: Store,
    pages: Vec<Box<dyn Page>>,
    active: usize,
    search: String,
    toast: Option<Toast>,
    /// A destructive action waiting for the user to confirm it.
    confirmation: Option<Confirmation>,
    /// The container being streamed, and whether it was running when the
    /// stream started (a restart needs a fresh stream).
    watching: Option<(String, bool)>,
    /// The ⌘K palette, while it's open.
    palette: Option<CommandPalette>,
    /// Labels of recently used palette entries, most recent first.
    recent: Vec<String>,
    show_help: bool,
    settings: settings::Settings,
    show_settings: bool,
    updater: crate::update::Updater,
    /// The menu bar / notification area icon, where there is one.
    tray: Option<tray::Tray>,
    /// Set by Quit in the tray, so closing really quits.
    quitting: bool,
    /// Starts the engine when it's down.
    engine_starter: engine_start::EngineStarter,
}

impl FerrodockApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::install(&cc.egui_ctx);
        let settings: settings::Settings = cc
            .storage
            .and_then(|s| eframe::get_value(s, settings::STORAGE_KEY))
            .unwrap_or_default();
        settings.apply(&cc.egui_ctx);
        let updater = {
            let ctx = cc.egui_ctx.clone();
            crate::update::Updater::new(move || ctx.request_repaint())
        };
        if settings.check_updates {
            updater.check();
        }
        let ctx = cc.egui_ctx.clone();
        let worker = Worker::spawn(move || ctx.request_repaint());
        let pages = pages::all();
        let mut store = Store::new();
        store.engines = crate::docker::engines();
        if let Some(engine) = crate::docker::pick(&store.engines, settings.engine.as_deref()) {
            store.engine = Some(engine.name.clone());
            worker.send(Request::UseEngine(engine));
        }
        // Load everything once so the sidebar counts are filled in.
        for page in &pages {
            worker.send(Request::Refresh(page.resource()));
        }
        // From now on the worker keeps the first page's data fresh by itself,
        // and the UI only wakes up when something changes.
        worker.send(Request::Poll(pages[0].resource()));
        Self {
            worker,
            store,
            pages,
            active: 0,
            search: String::new(),
            toast: None,
            confirmation: None,
            watching: None,
            palette: None,
            recent: Vec::new(),
            show_help: false,
            settings,
            show_settings: false,
            updater,
            tray: tray::Tray::new(&cc.egui_ctx),
            quitting: false,
            engine_starter: engine_start::EngineStarter::new(&cc.egui_ctx),
        }
        .started(&cc.egui_ctx)
    }

    /// Reads what only the system knows, and starts hidden in the tray when
    /// opened at login.
    fn started(mut self, ctx: &egui::Context) -> Self {
        self.settings.launch_at_login = autostart::enabled();
        if self.tray.is_some() && autostart::started_in_background() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }
        self
    }

    /// Brings the tray up to date, acts on its clicks, and hides the window
    /// instead of closing it when the tray keeps Ferrodock running.
    fn handle_tray(&mut self, ctx: &egui::Context) {
        let Some(tray) = &mut self.tray else { return };
        for action in tray.update(&self.store) {
            match action {
                tray::TrayAction::Open => show_window(ctx),
                tray::TrayAction::Show { page, id } => {
                    show_window(ctx);
                    self.handle_intent(Intent::Open { page, id });
                }
                tray::TrayAction::OpenPage(page) => {
                    show_window(ctx);
                    self.handle_intent(Intent::OpenPage(page));
                }
                tray::TrayAction::QuickStart(name) => {
                    if let Some(template) = crate::docker::template(name) {
                        let choices = pages::quick_start::defaults(&self.store, template);
                        self.handle_intent(Intent::RunImage(template.spec(&choices)));
                    }
                }
                tray::TrayAction::Run(tasks) => {
                    for task in tasks {
                        self.run(task);
                    }
                }
                tray::TrayAction::Quit => {
                    self.quitting = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
        if self.settings.close_to_tray
            && !self.quitting
            && ctx.input(|i| i.viewport().close_requested())
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }
    }

    /// Talks to `engine` from now on, and remembers it.
    fn switch_engine(&mut self, engine: crate::docker::Engine) {
        self.settings.engine = Some(engine.name.clone());
        self.store = Store::new();
        self.store.engines = crate::docker::engines();
        self.store.engine = Some(engine.name.clone());
        self.watching = None;
        self.engine_starter.engine_switched();
        self.worker.send(Request::UseEngine(engine));
        for page in &self.pages {
            self.worker.send(Request::Refresh(page.resource()));
        }
        self.worker
            .send(Request::Poll(self.pages[self.active].resource()));
    }

    /// Reloads the open page's data (plus the containers) right away.
    fn refresh(&mut self) {
        let resource = self.pages[self.active].resource();
        self.worker.send(Request::Refresh(resource));
    }

    fn open_page(&mut self, index: usize) {
        if index != self.active {
            self.pages[self.active].clear_selection();
            self.active = index;
            self.search.clear();
            // Loads the page's data now and keeps it fresh.
            self.worker
                .send(Request::Poll(self.pages[index].resource()));
        }
    }

    fn handle_responses(&mut self, ctx: &egui::Context) {
        // Hidden in the tray, or another app in front.
        let away = !ctx.input(|i| i.viewport().focused.unwrap_or(true));
        while let Some(response) = self.worker.try_recv() {
            match response {
                crate::worker::Response::EngineStarted(result) => {
                    self.engine_starter.finished(result);
                    continue;
                }
                crate::worker::Response::Provider(app) => {
                    self.settings.engine_app = Some(app);
                    continue;
                }
                _ => {}
            }
            if let Some(notice) = self.store.apply(response) {
                if notice.alert && away && self.settings.notify_crashes {
                    notify::send(notice.text.clone());
                }
                self.toast = Some(Toast::new(notice.text, notice.is_error));
            }
        }
    }

    /// Starts, switches or stops the live logs/stats stream to match what
    /// the open page wants to watch.
    fn sync_watch(&mut self) {
        let wanted = self.pages[self.active]
            .watched_container(&self.store)
            .map(|id| {
                let running = self.store.container(&id).is_some_and(|c| c.is_running());
                (id, running)
            });
        if wanted == self.watching {
            return;
        }
        let id = wanted.as_ref().map(|(id, _)| id.clone());
        self.store.live = id.clone().map(Live::new);
        self.worker.send(Request::Watch(id));
        self.watching = wanted;
    }

    /// Whether a dialog or overlay has the keyboard.
    fn overlay_open(&self) -> bool {
        self.palette.is_some()
            || self.pages[self.active].has_overlay()
            || self.show_help
            || self.show_settings
            || self.confirmation.is_some()
    }

    fn handle_shortcut(
        &mut self,
        ctx: &egui::Context,
        p: &'static theme::Palette,
        shortcut: Shortcut,
    ) {
        match shortcut {
            Shortcut::Palette => self.palette = Some(CommandPalette::default()),
            Shortcut::Help => self.show_help = true,
            Shortcut::Settings => self.show_settings = true,
            Shortcut::SelectAll => {
                let mut intents = Vec::new();
                let page_ctx = PageCtx::new(&self.store, p, &self.search, &mut intents);
                self.pages[self.active].select_all(&page_ctx);
            }
            Shortcut::Page(index) if index < self.pages.len() => self.open_page(index),
            Shortcut::Page(_) => {}
            Shortcut::Step(delta) => table::request_step(ctx, delta),
            Shortcut::CloseDetails => self.pages[self.active].clear_selection(),
            Shortcut::FocusSearch => ctx.memory_mut(|m| m.request_focus(shell::search_id())),
            Shortcut::Refresh => self.refresh(),
            Shortcut::PageKey(key) => {
                let mut intents = Vec::new();
                let mut page_ctx = PageCtx::new(&self.store, p, &self.search, &mut intents);
                self.pages[self.active].shortcut(key, &mut page_ctx);
                intents.into_iter().for_each(|i| self.handle_intent(i));
            }
        }
    }

    fn show_palette(&mut self, ui: &mut egui::Ui, p: &theme::Palette) {
        let Some(palette) = &mut self.palette else {
            return;
        };
        let (entries, mut targets): (Vec<_>, Vec<_>) =
            commands::build(&self.pages, &self.store, &self.recent)
                .into_iter()
                .map(|item| (item.entry, item.target))
                .unzip();
        match palette.show(ui, p, &entries) {
            Some(Outcome::Chosen(index)) => {
                self.palette = None;
                commands::remember(&mut self.recent, &entries[index].label);
                self.run_target(ui.ctx(), targets.swap_remove(index));
            }
            Some(Outcome::Closed) => self.palette = None,
            None => {}
        }
    }

    fn run_target(&mut self, ctx: &egui::Context, target: Target) {
        match target {
            Target::OpenPage(index) => self.open_page(index),
            Target::Page(index, run) => {
                self.open_page(index);
                match run {
                    CommandRun::Select(id) => {
                        self.pages[index].select(&id);
                        table::request_step(ctx, 0);
                    }
                    CommandRun::Intents(intents) => {
                        intents.into_iter().for_each(|i| self.handle_intent(i));
                    }
                }
            }
            Target::Refresh => self.refresh(),
            Target::Help => self.show_help = true,
            Target::Settings => self.show_settings = true,
        }
    }

    fn handle_intent(&mut self, intent: Intent) {
        match intent {
            Intent::Run(task) => self.run(task),
            Intent::Confirm(confirmation) => self.confirmation = Some(confirmation),
            Intent::Pull(image) => {
                self.store.pending.insert(image.clone(), Action::Pull);
                self.worker.send(Request::Pull(image));
            }
            Intent::RunImage(spec) => self.worker.send(Request::RunImage(spec)),
            Intent::Compose(op) => {
                if let Some(old) = self.store.compose.requested(&op) {
                    self.worker
                        .send(Request::Compose(crate::docker::ComposeOp::Cancel {
                            job: old,
                        }));
                }
                if let crate::docker::ComposeOp::Run { target, .. } = &op {
                    self.settings.remember_compose_files(&target.files);
                }
                self.worker.send(Request::Compose(op));
            }
            Intent::Image(op) => {
                use crate::docker::ImageOp;
                let busy = match &op {
                    ImageOp::Search(term) => {
                        self.store.search = Some((term.clone(), None));
                        None
                    }
                    ImageOp::Tag { image, .. } => Some((image.clone(), Action::Tag)),
                    ImageOp::Push(reference) => Some((reference.clone(), Action::Push)),
                    ImageOp::Save(image) => Some((image.clone(), Action::Save)),
                    ImageOp::Load(path) => Some((path.display().to_string(), Action::Load)),
                };
                if let Some((id, action)) = busy {
                    self.store.pending.insert(id, action);
                }
                self.worker.send(Request::Image(op));
            }
            Intent::Terminal(op) => {
                if let Some(old) = self.store.terminal_requested(&op) {
                    self.worker
                        .send(Request::Terminal(crate::docker::TermOp::Close {
                            session: old,
                        }));
                }
                self.worker.send(Request::Terminal(op));
            }
            Intent::Files(op) => {
                self.store.files_requested(&op);
                self.worker.send(Request::Files(op));
            }
            Intent::Change(change) => {
                let task = change.task();
                self.store.pending.insert(task.id, task.action);
                self.worker.send(Request::Change(change));
            }
            Intent::Processes(id) => {
                // Keep the old table on screen while the new one loads.
                let previous = self
                    .store
                    .processes
                    .take()
                    .filter(|(asked, _, _)| *asked == id)
                    .and_then(|(_, _, table)| table);
                self.store.processes = Some((id.clone(), std::time::Instant::now(), previous));
                self.worker.send(Request::Processes(id));
            }
            Intent::History(image) => {
                self.store.history = Some((image.clone(), None));
                self.worker.send(Request::History(image));
            }
            Intent::Notify { text, is_error } => self.toast = Some(Toast::new(text, is_error)),
            Intent::Open { page, id } => {
                if let Some(index) = self.pages.iter().position(|p| p.title() == page) {
                    self.open_page(index);
                    self.pages[index].select(&id);
                }
            }
            Intent::OpenPage(title) => {
                if let Some(index) = self.pages.iter().position(|p| p.title() == title) {
                    self.open_page(index);
                }
            }
            Intent::ClearSearch => self.search.clear(),
            Intent::Inspect(resource, id) => {
                self.store.inspection = None;
                self.store.inspecting = Some(id.clone());
                self.worker.send(Request::Inspect(resource, id));
            }
        }
    }

    fn run(&mut self, task: Task) {
        self.store.pending.insert(task.id.clone(), task.action);
        self.worker.send(Request::Run(task));
    }

    fn show_confirmation(&mut self, ui: &mut egui::Ui, p: &theme::Palette) {
        let Some(confirmation) = &mut self.confirmation else {
            return;
        };
        match confirm::show(ui, p, confirmation) {
            Some(Choice::Confirm) => {
                if let Some(confirmation) = self.confirmation.take() {
                    confirmation
                        .chosen_tasks()
                        .into_iter()
                        .for_each(|task| self.run(task));
                }
            }
            Some(Choice::Cancel) => self.confirmation = None,
            None => {}
        }
    }
}

impl eframe::App for FerrodockApp {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, settings::STORAGE_KEY, &self.settings);
    }

    /// Ferrodock's own engine runs inside Ferrodock: shut it down cleanly.
    fn on_exit(&mut self) {
        self.engine_starter.builtin.stop();
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.handle_responses(ui.ctx());
        if let Some(engine) = self
            .engine_starter
            .update(&self.store, &self.settings, &self.worker)
        {
            self.switch_engine(engine);
        }
        self.handle_tray(ui.ctx());
        self.store
            .recent_compose
            .clone_from(&self.settings.recent_compose);
        let p = theme::palette(ui);
        if !self.overlay_open() {
            for shortcut in keys::read(ui.ctx()) {
                self.handle_shortcut(ui.ctx(), p, shortcut);
            }
        }

        let clicked_page = egui::Panel::left("sidebar")
            .resizable(false)
            .exact_size(210.0)
            .frame(Frame::new().fill(p.sidebar).inner_margin(Margin::same(14)))
            .show(ui, |ui| {
                sidebar::show(
                    ui,
                    p,
                    &self.pages,
                    self.active,
                    &self.store,
                    &self.updater.status(),
                )
            })
            .inner;
        match clicked_page {
            Some(sidebar::Click::Page(index)) => self.open_page(index),
            Some(sidebar::Click::Settings) => self.show_settings = true,
            Some(sidebar::Click::Engine(engine)) => self.switch_engine(engine),
            Some(sidebar::Click::Update) => {
                if let crate::update::Status::Available(available) = self.updater.status() {
                    self.updater.install(available);
                }
            }
            Some(sidebar::Click::Restart) => {
                if let crate::update::Status::Ready { relaunch, .. } = self.updater.status() {
                    match relaunch.start() {
                        Ok(()) => ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close),
                        Err(e) => {
                            self.toast = Some(Toast::new(format!("Couldn't restart: {e}"), true));
                        }
                    }
                }
            }
            None => {}
        }

        let mut intents = Vec::new();
        let mut refresh = false;
        // Ferrodock's own engine, when starting it means switching to it.
        let mut switch_to = None;
        let page = &mut self.pages[self.active];

        if page.has_selection(&self.store) {
            egui::Panel::right("details")
                .default_size(340.0)
                .min_size(280.0)
                .frame(Frame::new().fill(p.sidebar).inner_margin(Margin::same(18)))
                .show(ui, |ui| {
                    let mut ctx = PageCtx::new(&self.store, p, &self.search, &mut intents);
                    page.show_details(ui, &mut ctx);
                });
        }

        egui::CentralPanel::default_margins()
            .frame(
                Frame::new()
                    .fill(p.bg)
                    .inner_margin(Margin::symmetric(22, 18)),
            )
            .show(ui, |ui| {
                let summary = page.summary(&self.store);
                // The toolbar sees this frame's search text; the box edits a copy.
                let mut search = self.search.clone();
                refresh = shell::header(
                    ui,
                    p,
                    page.title(),
                    &summary,
                    page.search_hint(),
                    &mut search,
                    |ui| {
                        if !self.store.is_disconnected() {
                            let mut ctx = PageCtx::new(&self.store, p, &self.search, &mut intents);
                            page.toolbar(ui, &mut ctx);
                        }
                    },
                );
                self.search = search;
                ui.add_space(12.0);
                if let Connection::Disconnected { reason } = &self.store.connection {
                    let offer = self.engine_starter.offer(&self.store, &self.settings);
                    match shell::disconnected(ui, p, reason, &offer) {
                        Some(shell::Click::Retry) => refresh = true,
                        Some(shell::Click::Start) => {
                            switch_to = self.engine_starter.start(
                                &self.store,
                                &self.settings,
                                &self.worker,
                            );
                        }
                        None => {}
                    }
                } else if matches!(self.store.connection, Connection::Connecting) {
                    shell::connecting(ui, p);
                } else {
                    let mut ctx = PageCtx::new(&self.store, p, &self.search, &mut intents);
                    page.show(ui, &mut ctx);
                }
            });

        for intent in intents {
            self.handle_intent(intent);
        }
        self.sync_watch();
        if refresh {
            self.refresh();
        }
        if let Some(engine) = switch_to {
            self.switch_engine(engine);
        }
        table::clear_step(ui.ctx());
        self.show_confirmation(ui, p);
        self.show_palette(ui, p);
        if self.show_settings {
            self.show_settings = settings::show(ui, p, &mut self.settings, &self.updater);
        }
        if self.show_help {
            let titles: Vec<&'static str> = self.pages.iter().map(|page| page.title()).collect();
            self.show_help = shortcuts::show(ui, p, &keys::help(&titles));
        }
        if let Some(toast) = &self.toast
            && !toast.show(ui, p)
        {
            self.toast = None;
        }
    }
}

/// Shows the window (hidden by closing it to the tray) and brings it to
/// the front.
fn show_window(ctx: &egui::Context) {
    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
}
