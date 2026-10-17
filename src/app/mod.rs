//! The IronStack window.
//!
//! This module only wires things together: it feeds worker responses into
//! the [`Store`], lays out the shell, lets the active [`Page`] draw itself
//! and turns the page's [`Intent`]s into worker requests.

mod components;
mod page;
mod pages;
mod shell;
mod sidebar;
mod store;

use std::time::{Duration, Instant};

use eframe::egui::{self, Frame, Margin};

use crate::docker::{Resource, Task};
use crate::theme;
use crate::worker::{Request, Worker};
use components::confirm::{self, Choice};
use components::toast::Toast;
use page::{Confirmation, Intent, Page, PageCtx};
use store::{Connection, Store};

const REFRESH_INTERVAL: Duration = Duration::from_secs(3);

pub struct IronStackApp {
    worker: Worker,
    store: Store,
    pages: Vec<Box<dyn Page>>,
    active: usize,
    search: String,
    last_refresh: Instant,
    toast: Option<Toast>,
    /// A destructive action waiting for the user to confirm it.
    confirmation: Option<Confirmation>,
}

impl IronStackApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::install(&cc.egui_ctx);
        let ctx = cc.egui_ctx.clone();
        let worker = Worker::spawn(move || ctx.request_repaint());
        let pages = pages::all();
        // Load everything once so the sidebar counts are filled in.
        for page in &pages {
            worker.send(Request::Refresh(page.resource()));
        }
        Self {
            worker,
            store: Store::new(),
            pages,
            active: 0,
            search: String::new(),
            last_refresh: Instant::now(),
            toast: None,
            confirmation: None,
        }
    }

    /// Reloads the containers (they drive the connection status and the
    /// "in use" info on every page) plus the open page's data.
    fn refresh(&mut self) {
        self.last_refresh = Instant::now();
        self.worker.send(Request::Refresh(Resource::Containers));
        let resource = self.pages[self.active].resource();
        if resource != Resource::Containers {
            self.worker.send(Request::Refresh(resource));
        }
    }

    fn open_page(&mut self, index: usize) {
        if index != self.active {
            self.pages[self.active].clear_selection();
            self.active = index;
            self.search.clear();
            self.refresh();
        }
    }

    fn handle_responses(&mut self) {
        while let Some(response) = self.worker.try_recv() {
            if let Some(notice) = self.store.apply(response) {
                self.toast = Some(Toast::new(notice.text, notice.is_error));
            }
        }
    }

    fn handle_intent(&mut self, intent: Intent) {
        match intent {
            Intent::Run(task) => self.run(task),
            Intent::Confirm(confirmation) => self.confirmation = Some(confirmation),
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

impl eframe::App for IronStackApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.handle_responses();
        if ui.input(|i| i.key_pressed(egui::Key::F5))
            || self.last_refresh.elapsed() >= REFRESH_INTERVAL
        {
            self.refresh();
        }
        ui.ctx().request_repaint_after(REFRESH_INTERVAL);

        let p = theme::palette(ui);
        let clicked_page = egui::Panel::left("sidebar")
            .resizable(false)
            .exact_size(210.0)
            .frame(Frame::new().fill(p.sidebar).inner_margin(Margin::same(14)))
            .show(ui, |ui| {
                sidebar::show(ui, p, &self.pages, self.active, &self.store)
            })
            .inner;
        if let Some(index) = clicked_page {
            self.open_page(index);
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
        if refresh {
            self.refresh();
        }
        self.show_confirmation(ui, p);
        if let Some(toast) = &self.toast
            && !toast.show(ui, p)
        {
            self.toast = None;
        }
    }
}
