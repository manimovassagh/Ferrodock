//! The Ferrodock window.
//!
//! This module only wires things together: it feeds worker responses into
//! the [`Store`], lays out the shell, lets the active [`Page`] draw itself
//! and turns the page's [`Intent`]s into worker requests.

mod commands;
mod components;
mod keys;
mod page;
mod pages;
#[cfg(test)]
mod screenshots;
mod settings;
mod shell;
mod sidebar;
mod store;

use eframe::egui::{self, Frame, Margin};

use crate::docker::Task;
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
}

impl FerrodockApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::install(&cc.egui_ctx);
        let settings: settings::Settings = cc
            .storage
            .and_then(|s| eframe::get_value(s, settings::STORAGE_KEY))
            .unwrap_or_default();
        settings.apply(&cc.egui_ctx);
        let ctx = cc.egui_ctx.clone();
        let worker = Worker::spawn(move || ctx.request_repaint());
        let pages = pages::all();
        // Load everything once so the sidebar counts are filled in.
        for page in &pages {
            worker.send(Request::Refresh(page.resource()));
        }
        // From now on the worker keeps the first page's data fresh by itself,
        // and the UI only wakes up when something changes.
        worker.send(Request::Poll(pages[0].resource()));
        Self {
            worker,
            store: Store::new(),
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
        }
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

    fn handle_responses(&mut self) {
        while let Some(response) = self.worker.try_recv() {
            if let Some(notice) = self.store.apply(response) {
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
                self.store.pending.insert(image.clone());
                self.worker.send(Request::Pull(image));
            }
            Intent::RunImage(spec) => self.worker.send(Request::RunImage(spec)),
            Intent::Inspect(resource, id) => {
                self.store.inspection = None;
                self.store.inspecting = Some(id.clone());
                self.worker.send(Request::Inspect(resource, id));
            }
        }
    }

    fn run(&mut self, task: Task) {
        self.store.pending.insert(task.id.clone());
        self.worker.send(Request::Run(task));
    }

    fn show_confirmation(&mut self, ui: &mut egui::Ui, p: &theme::Palette) {
        let Some(confirmation) = &self.confirmation else {
            return;
        };
        match confirm::show(ui, p, confirmation) {
            Some(Choice::Confirm) => {
                if let Some(confirmation) = self.confirmation.take() {
                    confirmation
                        .tasks
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

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.handle_responses();
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
                sidebar::show(ui, p, &self.pages, self.active, &self.store)
            })
            .inner;
        match clicked_page {
            Some(sidebar::Click::Page(index)) => self.open_page(index),
            Some(sidebar::Click::Settings) => self.show_settings = true,
            None => {}
        }

        let mut intents = Vec::new();
        let mut refresh = false;
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
                    refresh |= shell::disconnected(ui, p, reason);
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
        table::clear_step(ui.ctx());
        self.show_confirmation(ui, p);
        self.show_palette(ui, p);
        if self.show_settings {
            self.show_settings = settings::show(ui, p, &mut self.settings);
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
