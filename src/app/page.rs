//! The contract every sidebar page implements.
//!
//! The window shell (sidebar, header, details pane) only knows about this
//! trait, so adding a page means writing one new type and registering it in
//! [`crate::app::pages::all`] — nothing else changes.

use eframe::egui;

use super::store::Store;
use crate::docker::{Resource, Task};
use crate::theme::Palette;

/// Something a page wants done. The app decides how (today: by asking the
/// background worker), so pages never depend on the worker directly.
#[derive(Debug, Clone, PartialEq)]
pub enum Intent {
    /// Do it now.
    Run(Task),
    /// Ask the user first; run the tasks only if they agree.
    Confirm(Confirmation),
}

/// A question for the user before something destructive happens.
#[derive(Debug, Clone, PartialEq)]
pub struct Confirmation {
    pub title: String,
    pub message: String,
    /// Label of the confirm button, e.g. "Remove".
    pub action_label: String,
    pub tasks: Vec<Task>,
}

/// What a page gets to work with while drawing.
pub struct PageCtx<'a> {
    pub store: &'a Store,
    pub palette: &'static Palette,
    search: &'a str,
    intents: &'a mut Vec<Intent>,
}

impl<'a> PageCtx<'a> {
    pub fn new(
        store: &'a Store,
        palette: &'static Palette,
        search: &'a str,
        intents: &'a mut Vec<Intent>,
    ) -> Self {
        Self {
            store,
            palette,
            search,
            intents,
        }
    }

    pub fn request(&mut self, intent: Intent) {
        self.intents.push(intent);
    }

    /// Whether any of `fields` contains the search text (case-insensitive).
    pub fn matches_search(&self, fields: &[&str]) -> bool {
        let needle = self.search.trim().to_lowercase();
        needle.is_empty() || fields.iter().any(|f| f.to_lowercase().contains(&needle))
    }
}

pub trait Page {
    fn title(&self) -> &'static str;

    /// The data this page shows; it's refreshed while the page is open.
    fn resource(&self) -> Resource;

    /// Number shown next to the page in the sidebar.
    fn count(&self, store: &Store) -> usize;

    /// One-line summary under the page title, e.g. "3 running · 4 total".
    fn summary(&self, store: &Store) -> String;

    fn search_hint(&self) -> &'static str {
        "Search"
    }

    /// Extra buttons for the page header, e.g. "Remove unused".
    fn toolbar(&mut self, _ui: &mut egui::Ui, _ctx: &mut PageCtx) {}

    /// Draws the page body below the shared header.
    fn show(&mut self, ui: &mut egui::Ui, ctx: &mut PageCtx);

    /// Whether an item is selected *and* still exists, i.e. whether the
    /// details pane should be open.
    fn has_selection(&self, store: &Store) -> bool;

    /// Draws the details pane for the selected item.
    fn show_details(&mut self, ui: &mut egui::Ui, ctx: &mut PageCtx);

    fn clear_selection(&mut self);
}

/// Shared click behaviour: a click opens the row, a second click closes it.
pub fn toggle_selection(selected: &mut Option<String>, id: String) {
    *selected = if selected.as_ref() == Some(&id) {
        None
    } else {
        Some(id)
    };
}
