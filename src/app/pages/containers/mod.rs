//! Containers page: state tabs, the container table, and a details pane
//! with Overview, Logs and Stats tabs.

mod actions;
mod details;
mod logs;
mod stats;
mod status;
mod table;

use eframe::egui;

use crate::app::components::widgets;
use crate::app::page::{Page, PageCtx};
use crate::app::store::Store;
use crate::docker::{ContainerInfo, Resource};
use crate::theme::Palette;

#[derive(Clone, Copy, PartialEq, Eq)]
enum StateFilter {
    All,
    Running,
    Stopped,
}

impl StateFilter {
    fn matches(self, c: &ContainerInfo) -> bool {
        match self {
            StateFilter::All => true,
            StateFilter::Running => c.is_active(),
            StateFilter::Stopped => !c.is_active(),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DetailsTab {
    Overview,
    Logs,
    Stats,
}

pub struct ContainersPage {
    filter: StateFilter,
    selected: Option<String>,
    tab: DetailsTab,
    log_filter: String,
}

impl ContainersPage {
    pub fn new() -> Self {
        Self {
            filter: StateFilter::All,
            selected: None,
            tab: DetailsTab::Overview,
            log_filter: String::new(),
        }
    }

    fn selected<'a>(&self, store: &'a Store) -> Option<&'a ContainerInfo> {
        self.selected.as_deref().and_then(|id| store.container(id))
    }

    fn state_tabs(&mut self, ui: &mut egui::Ui, p: &Palette, store: &Store) {
        let total = store.containers.len();
        let running = store.running_count();
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            for (filter, label, count) in [
                (StateFilter::All, "All", total),
                (StateFilter::Running, "Running", running),
                (StateFilter::Stopped, "Stopped", total - running),
            ] {
                if widgets::tab(ui, p, label, Some(count), self.filter == filter).clicked() {
                    self.filter = filter;
                }
            }
        });
    }
}

impl Page for ContainersPage {
    fn title(&self) -> &'static str {
        "Containers"
    }

    fn resource(&self) -> Resource {
        Resource::Containers
    }

    fn count(&self, store: &Store) -> usize {
        store.containers.len()
    }

    fn summary(&self, store: &Store) -> String {
        format!(
            "{} running · {} total",
            store.running_count(),
            store.containers.len()
        )
    }

    fn search_hint(&self) -> &'static str {
        "Search name, image or ID"
    }

    fn show(&mut self, ui: &mut egui::Ui, ctx: &mut PageCtx) {
        let p = ctx.palette;
        let store = ctx.store;
        self.state_tabs(ui, p, store);
        ui.add_space(8.0);

        let visible: Vec<&ContainerInfo> = store
            .containers
            .iter()
            .filter(|c| self.filter.matches(c))
            .filter(|c| ctx.matches_search(&[&c.name, &c.image, c.short_id()]))
            .collect();
        if visible.is_empty() {
            let message = if store.containers.is_empty() {
                "No containers yet. Create one with `docker run` and it will show up here."
            } else {
                "No containers match."
            };
            widgets::empty_state(ui, p, message);
            return;
        }
        table::show(self, ui, ctx, &visible);
    }

    fn has_selection(&self, store: &Store) -> bool {
        self.selected(store).is_some()
    }

    fn watched_container(&self, store: &Store) -> Option<String> {
        self.selected(store).map(|c| c.id.clone())
    }

    fn show_details(&mut self, ui: &mut egui::Ui, ctx: &mut PageCtx) {
        details::show(self, ui, ctx);
    }

    fn clear_selection(&mut self) {
        self.selected = None;
    }
}
