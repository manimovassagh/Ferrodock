//! The contract every sidebar page implements.
//!
//! The window shell (sidebar, header, details pane) only knows about this
//! trait, so adding a page means writing one new type and registering it in
//! [`crate::app::pages::all`] — nothing else changes.

use eframe::egui;

use super::components::icons::Icon;
use super::store::Store;
use crate::docker::{
    Change, ComposeOp, FileOp, ImageOp, MachineSpec, Resource, RunSpec, Task, TermOp,
};
use crate::theme::Palette;

/// Something a page wants done. The app decides how (today: by asking the
/// background worker), so pages never depend on the worker directly.
#[derive(Debug, Clone, PartialEq)]
pub enum Intent {
    /// Do it now.
    Run(Task),
    /// Ask the user first; run the tasks only if they agree.
    Confirm(Confirmation),
    /// Load the full inspect output of an item (by the ID the Engine uses).
    Inspect(Resource, String),
    /// Switch to the page with this title.
    OpenPage(&'static str),
    /// Switch to the page with this title and select an item there.
    Open { page: &'static str, id: String },
    /// Empty the search box.
    ClearSearch,
    /// Tell the user something, e.g. where a file was saved.
    Notify { text: String, is_error: bool },
    /// Create a volume or network, or (dis)connect a container.
    Change(Change),
    /// Load (or reload) the process table of a container.
    Processes(String),
    /// Load the layers of an image.
    History(String),
    /// Pull an image by reference, e.g. `nginx:alpine`.
    Pull(String),
    /// Create and start a container.
    RunImage(RunSpec),
    /// Create, start and set up a Linux machine.
    CreateMachine(Box<MachineSpec>),
    /// List, preview or copy files of a container.
    Files(FileOp),
    /// Open, type into, resize or close a shell.
    Terminal(TermOp),
    /// Search, tag, push, save or load images.
    Image(ImageOp),
    /// Run `docker compose`.
    Compose(ComposeOp),
    /// Pick a Compose file and bring its project up.
    OpenComposeFile,
}

/// A question for the user before something destructive happens.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Confirmation {
    pub title: String,
    pub message: String,
    /// Label of the confirm button, e.g. "Remove".
    pub action_label: String,
    pub tasks: Vec<Task>,
    /// A box to tick that runs other tasks instead, e.g. "Stop running
    /// ones first".
    pub option: Option<ConfirmOption>,
    /// A word the user has to type before the button turns on, for things
    /// that are hard to undo.
    pub typed: Option<TypedCheck>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ConfirmOption {
    pub label: String,
    /// Shown under the label.
    pub hint: String,
    pub on: bool,
    /// Run instead of the confirmation's tasks while ticked.
    pub tasks: Vec<Task>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct TypedCheck {
    pub word: String,
    /// What the user typed so far.
    pub entered: String,
}

impl TypedCheck {
    pub fn new(word: &str) -> Self {
        Self {
            word: word.to_owned(),
            entered: String::new(),
        }
    }

    pub fn matches(&self) -> bool {
        self.entered.trim() == self.word
    }
}

impl Confirmation {
    /// The tasks to run, given the option's state.
    pub fn chosen_tasks(self) -> Vec<Task> {
        match self.option {
            Some(option) if option.on => option.tasks,
            _ => self.tasks,
        }
    }

    /// Whether the confirm button may be pressed.
    pub fn ready(&self) -> bool {
        self.typed.as_ref().is_none_or(TypedCheck::matches)
    }
}

/// An entry a page offers in the command palette.
pub struct Command {
    pub label: String,
    /// Dimmed text after the label, e.g. "Container · running".
    pub detail: String,
    pub run: CommandRun,
}

pub enum CommandRun {
    /// Open the page and select this item (by the ID the page uses).
    Select(String),
    /// Hand these to the app, like a click on a button would.
    Intents(Vec<Intent>),
}

impl Command {
    pub fn select(
        label: impl Into<String>,
        detail: impl Into<String>,
        id: impl Into<String>,
    ) -> Self {
        Self {
            label: label.into(),
            detail: detail.into(),
            run: CommandRun::Select(id.into()),
        }
    }

    pub fn intent(label: impl Into<String>, detail: impl Into<String>, intent: Intent) -> Self {
        Self {
            label: label.into(),
            detail: detail.into(),
            run: CommandRun::Intents(vec![intent]),
        }
    }
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

    pub fn search_is_empty(&self) -> bool {
        self.search.trim().is_empty()
    }

    /// Whether any of `fields` contains the search text (case-insensitive).
    pub fn matches_search(&self, fields: &[&str]) -> bool {
        let needle = self.search.trim().to_lowercase();
        needle.is_empty() || fields.iter().any(|f| f.to_lowercase().contains(&needle))
    }
}

pub trait Page {
    fn title(&self) -> &'static str;

    /// Shown next to the title in the sidebar.
    fn icon(&self) -> Icon;

    /// The data this page shows; it's refreshed while the page is open.
    fn resource(&self) -> Resource;

    /// Shown next to the page in the sidebar, usually how many items it has.
    fn badge(&self, store: &Store) -> String;

    /// One-line summary under the page title, e.g. "3 running · 4 total".
    fn summary(&self, store: &Store) -> String;

    fn search_hint(&self) -> &'static str {
        "Search"
    }

    /// Listed at the bottom of the sidebar, under START, rather than with
    /// the Docker pages.
    fn bottom(&self) -> bool {
        false
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

    /// Selects an item by ID, e.g. when it's picked in the command palette.
    fn select(&mut self, id: &str);

    /// Marks every item shown, for bulk actions (Cmd/Ctrl+A).
    fn select_all(&mut self, _ctx: &PageCtx) {}

    /// What this page offers in the command palette.
    fn commands(&self, _store: &Store) -> Vec<Command> {
        Vec::new()
    }

    /// Whether the page has a dialog open, so window-wide shortcuts should
    /// leave the keyboard alone.
    fn has_overlay(&self) -> bool {
        false
    }

    /// A single-key shortcut pressed while this page is open and no text
    /// field has focus, e.g. `S` to start or stop the selected container.
    fn shortcut(&mut self, _key: egui::Key, _ctx: &mut PageCtx) {}

    /// A container whose logs and stats should stream while this page is
    /// open (the worker follows at most one at a time).
    fn watched_container(&self, _store: &Store) -> Option<String> {
        None
    }
}

/// Shared click behaviour: a click opens the row, a second click closes it.
pub fn toggle_selection(selected: &mut Option<String>, id: String) {
    *selected = if selected.as_ref() == Some(&id) {
        None
    } else {
        Some(id)
    };
}
