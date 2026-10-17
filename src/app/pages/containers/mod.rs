//! Containers page: state tabs, the container table, and a details pane
//! with Overview, Logs and Stats tabs.

mod actions;
mod details;
mod logs;
pub(super) mod ports;
mod processes;
mod stats;
mod status;
mod table;

use eframe::egui;

use super::run_dialog::RunForm;
use super::{confirm_removal, remove_task};
use crate::app::components::icons::Icon;
use crate::app::components::widgets;
use crate::app::page::{Command, Intent, Page, PageCtx};
use crate::app::store::Store;
use crate::docker::{Action, ContainerInfo, Resource};
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
    Processes,
    Inspect,
}

pub struct ContainersPage {
    filter: StateFilter,
    selected: Option<String>,
    tab: DetailsTab,
    log_view: logs::LogView,
    /// Rows picked with Cmd/Ctrl- or Shift-click, for bulk actions.
    marked: std::collections::BTreeSet<String>,
    /// The row Shift-click extends from.
    anchor: Option<String>,
    /// The Duplicate dialog, while it's open.
    run_form: Option<RunForm>,
    /// The new name being typed in the Rename dialog, while it's open.
    rename: Option<String>,
}

impl ContainersPage {
    pub fn new() -> Self {
        Self {
            filter: StateFilter::All,
            selected: None,
            tab: DetailsTab::Overview,
            log_view: logs::LogView::default(),
            marked: Default::default(),
            anchor: None,
            run_form: None,
            rename: None,
        }
    }

    /// The containers the list shows, after the state filter and search.
    fn visible<'a>(&self, ctx: &PageCtx<'a>) -> Vec<&'a ContainerInfo> {
        ctx.store
            .containers
            .iter()
            .filter(|c| self.filter.matches(c))
            .filter(|c| ctx.matches_search(&[&c.name, &c.image, c.short_id()]))
            .collect()
    }

    fn selected<'a>(&self, store: &'a Store) -> Option<&'a ContainerInfo> {
        self.selected.as_deref().and_then(|id| store.container(id))
    }

    fn state_tabs(&mut self, ui: &mut egui::Ui, p: &Palette, store: &Store) {
        let total = store.containers.len();
        let running = store.running_count();
        widgets::segmented(ui, p, |ui| {
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

    fn icon(&self) -> Icon {
        Icon::Container
    }

    fn resource(&self) -> Resource {
        Resource::Containers
    }

    fn badge(&self, store: &Store) -> String {
        store.containers.len().to_string()
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

        let visible = self.visible(ctx);
        if visible.is_empty() {
            let exists = !store.containers.is_empty();
            if exists && ctx.search_is_empty() {
                // Only the Running/Stopped filter hides everything.
                let hint = "Nothing matches this filter.";
                if widgets::empty_state(ui, p, Icon::Search, "No matches", hint, Some("Show all")) {
                    self.filter = StateFilter::All;
                }
                return;
            }
            super::nothing_to_show(
                ui,
                ctx,
                exists,
                (
                    Icon::Container,
                    "No containers yet",
                    "Run an image to create one, or use `docker run` in a terminal.",
                ),
                Some(("Browse images", Intent::OpenPage("Images"))),
            );
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
        self.marked.clear();
    }

    fn has_overlay(&self) -> bool {
        self.run_form.is_some() || self.rename.is_some()
    }

    fn select_all(&mut self, ctx: &PageCtx) {
        self.marked = self.visible(ctx).iter().map(|c| c.id.clone()).collect();
        self.selected = None;
    }

    fn select(&mut self, id: &str) {
        self.selected = Some(id.to_owned());
        // Make sure the list shows it.
        self.filter = StateFilter::All;
    }

    fn commands(&self, store: &Store) -> Vec<Command> {
        let mut list = Vec::new();
        for c in &store.containers {
            list.push(Command::select(
                &c.name,
                format!("Container · {}", c.state),
                &c.id,
            ));
            for (action, _, label) in actions::available(c) {
                list.push(Command::intent(
                    format!("{label} {}", c.name),
                    "Container",
                    actions::intent(action, c),
                ));
            }
        }
        list
    }

    fn shortcut(&mut self, key: egui::Key, ctx: &mut PageCtx) {
        let Some(c) = self.selected(ctx.store) else {
            return;
        };
        if ctx.store.pending.contains_key(&c.id) {
            return;
        }
        let available = actions::available(c);
        let find = |wanted: &[Action]| {
            available
                .iter()
                .map(|(action, _, _)| *action)
                .find(|action| wanted.contains(action))
        };
        let action = match key {
            egui::Key::S => find(&[Action::Start, Action::Stop]),
            egui::Key::R => find(&[Action::Restart]),
            egui::Key::P => find(&[Action::Pause, Action::Unpause]),
            egui::Key::L => {
                self.tab = DetailsTab::Logs;
                None
            }
            egui::Key::Delete | egui::Key::Backspace if !c.is_active() => {
                let task = remove_task(Resource::Containers, &c.id, &c.name);
                ctx.request(confirm_removal("container", vec![task]));
                None
            }
            _ => None,
        };
        if let Some(action) = action {
            ctx.request(actions::intent(action, c));
        }
    }
}
