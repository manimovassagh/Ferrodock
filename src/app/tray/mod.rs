//! The menu bar (macOS) / notification area (Windows) icon: how many
//! containers run, and a menu to start, stop and restart them without
//! opening the window.
//!
//! The menu is described here as plain data, built from the store, so it
//! can be tested anywhere; `native` turns it into a real menu on the
//! platforms that have one. Linux has no tray yet (it would need GTK).

#[cfg(any(target_os = "macos", target_os = "windows"))]
mod native;

use crate::app::store::{Connection, Store};
use crate::docker::{Action, ContainerInfo, TEMPLATES, Task};

/// Items listed per section; the rest are a click away in the window.
const MAX_ITEMS: usize = 20;

/// One menu entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    Item {
        id: String,
        label: String,
        enabled: bool,
    },
    Separator,
    Menu {
        label: String,
        items: Vec<Entry>,
    },
}

/// What a click in the menu asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrayAction {
    /// Show the window.
    Open,
    /// Show the window with an item selected on a page.
    Show {
        page: &'static str,
        id: String,
    },
    /// Show the window on a page.
    OpenPage(&'static str),
    /// Run the Quick start template with this name, with its defaults.
    QuickStart(&'static str),
    Run(Vec<Task>),
    Quit,
}

fn item(id: impl Into<String>, label: impl Into<String>) -> Entry {
    Entry::Item {
        id: id.into(),
        label: label.into(),
        enabled: true,
    }
}

fn note(label: impl Into<String>) -> Entry {
    Entry::Item {
        id: String::new(),
        label: label.into(),
        enabled: false,
    }
}

/// The text next to the icon (macOS) and in its tooltip.
pub fn title(store: &Store) -> String {
    match store.connection {
        Connection::Connected { .. } => store.running_count().to_string(),
        _ => "–".into(),
    }
}

/// The whole menu, from what the store knows.
pub fn model(store: &Store) -> Vec<Entry> {
    let mut menu = vec![item("open", "Open Ferrodock"), Entry::Separator];
    match &store.connection {
        Connection::Connected { version } => {
            menu.push(note(format!(
                "Docker {version} · {} running",
                store.running_count()
            )));
        }
        _ => menu.push(note("Docker isn't running")),
    }

    // The same sections as the sidebar.
    menu.push(Entry::Separator);
    let mut containers: Vec<&ContainerInfo> = store.containers.iter().collect();
    // Running first, then by name (the store's order).
    containers.sort_by_key(|c| !c.is_active());
    let mut items: Vec<Entry> = containers
        .iter()
        .take(MAX_ITEMS)
        .map(|c| Entry::Menu {
            label: format!("{} {}", dot(c.is_active()), c.name),
            items: container_items(c),
        })
        .collect();
    if containers.len() > MAX_ITEMS {
        items.push(note(format!(
            "{} more in the window",
            containers.len() - MAX_ITEMS
        )));
    }
    menu.push(section("Containers", items));

    if !store.projects.is_empty() {
        let projects = store
            .projects
            .iter()
            .map(|project| {
                let running = project.running();
                let mut items = vec![note(format!(
                    "{running} of {} running",
                    project.containers.len()
                ))];
                if running < project.containers.len() {
                    items.push(item(format!("up:{}", project.name), "Start all"));
                }
                if running > 0 {
                    items.push(item(format!("down:{}", project.name), "Stop all"));
                }
                Entry::Menu {
                    label: format!("{} {}", dot(running > 0), project.name),
                    items,
                }
            })
            .collect();
        menu.push(Entry::Menu {
            label: "Compose".into(),
            items: projects,
        });
    }
    let images = store
        .images
        .iter()
        .map(|i| item(format!("image:{}", i.id), i.name()));
    menu.push(section("Images", listed(images.collect())));
    let volumes = store
        .volumes
        .iter()
        .map(|v| item(format!("volume:{}", v.name), &v.name));
    menu.push(section("Volumes", listed(volumes.collect())));
    // Your own networks: Docker's built-in ones are always there.
    let networks = store
        .networks
        .iter()
        .filter(|n| !n.is_builtin())
        .map(|n| item(format!("network:{}", n.id), &n.name));
    menu.push(section("Networks", listed(networks.collect())));

    menu.push(Entry::Separator);
    let mut templates: Vec<Entry> = TEMPLATES
        .iter()
        .map(|t| {
            let label = if crate::app::pages::quick_start::running(store, t).is_some() {
                format!("● {}", t.name)
            } else {
                t.name.to_owned()
            };
            item(format!("quick:{}", t.name), label)
        })
        .collect();
    templates.extend([Entry::Separator, item("quick-start", "Customize…")]);
    menu.push(Entry::Menu {
        label: "Quick start".into(),
        items: templates,
    });

    menu.extend([Entry::Separator, item("quit", "Quit Ferrodock")]);
    menu
}

/// A submenu, or a dimmed line when there's nothing in it.
fn section(label: &str, items: Vec<Entry>) -> Entry {
    if items.is_empty() {
        return note(format!("No {}", label.to_lowercase()));
    }
    Entry::Menu {
        label: label.into(),
        items,
    }
}

/// The first [`MAX_ITEMS`] of `items`, with a note for the rest.
fn listed(mut items: Vec<Entry>) -> Vec<Entry> {
    if items.len() > MAX_ITEMS {
        let rest = items.len() - MAX_ITEMS;
        items.truncate(MAX_ITEMS);
        items.push(note(format!("{rest} more in the window")));
    }
    items
}

fn dot(running: bool) -> &'static str {
    if running { "●" } else { "○" }
}

fn container_items(c: &ContainerInfo) -> Vec<Entry> {
    let mut items = Vec::new();
    if c.is_active() {
        items.push(item(format!("stop:{}", c.id), "Stop"));
        items.push(item(format!("restart:{}", c.id), "Restart"));
    } else {
        items.push(item(format!("start:{}", c.id), "Start"));
    }
    items.push(Entry::Separator);
    items.push(item(format!("show:{}", c.id), "Show in Ferrodock"));
    items
}

/// What the menu item `id` asks for, or None if what it refers to is gone.
pub fn action(store: &Store, id: &str) -> Option<TrayAction> {
    let task = |action: Action, c: &ContainerInfo| Task {
        action,
        id: c.id.clone(),
        name: c.name.clone(),
    };
    match id.split_once(':') {
        None => match id {
            "open" => Some(TrayAction::Open),
            "quit" => Some(TrayAction::Quit),
            "quick-start" => Some(TrayAction::OpenPage("Quick start")),
            _ => None,
        },
        Some(("quick", name)) => {
            crate::docker::template(name).map(|t| TrayAction::QuickStart(t.name))
        }
        Some((kind @ ("image" | "volume" | "network"), id)) => {
            let page = match kind {
                "image" => "Images",
                "volume" => "Volumes",
                _ => "Networks",
            };
            Some(TrayAction::Show {
                page,
                id: id.to_owned(),
            })
        }
        Some((verb @ ("up" | "down"), name)) => {
            let project = store.projects.iter().find(|p| p.name == name)?;
            let (action, wanted) = if verb == "up" {
                (Action::Start, false)
            } else {
                (Action::Stop, true)
            };
            let tasks = project
                .containers
                .iter()
                .filter(|c| c.is_active() == wanted)
                .map(|c| task(action, c))
                .collect();
            Some(TrayAction::Run(tasks))
        }
        Some((verb, container)) => {
            let c = store.container(container)?;
            let action = match verb {
                "start" => Action::Start,
                "stop" => Action::Stop,
                "restart" => Action::Restart,
                "show" => {
                    return Some(TrayAction::Show {
                        page: "Containers",
                        id: c.id.clone(),
                    });
                }
                _ => return None,
            };
            Some(TrayAction::Run(vec![task(action, c)]))
        }
    }
}

/// The icon and its menu, where the platform has one.
pub struct Tray {
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    native: native::NativeTray,
    /// The menu last shown, to rebuild it only when it changes.
    shown: Vec<Entry>,
    title: String,
}

impl Tray {
    /// Whether this platform has a tray.
    pub const SUPPORTED: bool = cfg!(any(target_os = "macos", target_os = "windows"));

    /// Adds the icon. None where there's no tray, or if it can't be made.
    pub fn new(ctx: &eframe::egui::Context) -> Option<Self> {
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        {
            match native::NativeTray::new(ctx) {
                Ok(native) => Some(Self {
                    native,
                    shown: Vec::new(),
                    title: String::new(),
                }),
                Err(e) => {
                    eprintln!("ferrodock: no tray icon: {e}");
                    None
                }
            }
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            let _ = ctx;
            None
        }
    }

    /// Brings the icon and menu up to date, and returns what was clicked.
    pub fn update(&mut self, store: &Store) -> Vec<TrayAction> {
        let menu = model(store);
        let title = title(store);
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        {
            if menu != self.shown {
                self.native.set_menu(&menu);
            }
            if title != self.title {
                self.native.set_title(&title);
            }
        }
        self.shown = menu;
        self.title = title;
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        let clicked = self.native.clicked();
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        let clicked: Vec<String> = Vec::new();
        clicked.iter().filter_map(|id| action(store, id)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::docker::ContainerInfo;

    fn container(id: &str, name: &str, running: bool) -> ContainerInfo {
        ContainerInfo {
            id: id.into(),
            name: name.into(),
            state: if running { "running" } else { "exited" }.into(),
            ..Default::default()
        }
    }

    fn store() -> Store {
        let mut store = Store::new();
        store.connection = Connection::Connected {
            version: "29.0".into(),
        };
        store.containers = vec![container("a1", "api", false), container("w1", "web", true)];
        store
    }

    #[test]
    fn lists_running_containers_first() {
        let menu = model(&store());
        let labels = |entries: &[Entry]| -> Vec<String> {
            entries
                .iter()
                .filter_map(|e| match e {
                    Entry::Menu { label, .. } => Some(label.clone()),
                    _ => None,
                })
                .collect()
        };
        // The sidebar's sections; empty ones are a dimmed line.
        assert_eq!(labels(&menu), ["Containers", "Quick start"]);
        assert!(menu.contains(&note("No images")));
        let Some(Entry::Menu { items, .. }) = menu
            .iter()
            .find(|e| matches!(e, Entry::Menu { label, .. } if label == "Containers"))
        else {
            panic!("expected a Containers submenu");
        };
        assert_eq!(labels(items), ["● web", "○ api"]);
        assert!(menu.contains(&note("Docker 29.0 · 1 running")));
        assert_eq!(title(&store()), "1");
        assert_eq!(
            action(&store(), "quick:Postgres"),
            Some(TrayAction::QuickStart("Postgres"))
        );
    }

    #[test]
    fn clicks_become_actions() {
        let store = store();
        assert_eq!(action(&store, "open"), Some(TrayAction::Open));
        let Some(TrayAction::Run(tasks)) = action(&store, "stop:w1") else {
            panic!("stop should run a task");
        };
        assert_eq!(
            (tasks[0].action, tasks[0].name.as_str()),
            (Action::Stop, "web")
        );
        assert_eq!(
            action(&store, "show:a1"),
            Some(TrayAction::Show {
                page: "Containers",
                id: "a1".into()
            })
        );
        assert_eq!(action(&store, "stop:gone"), None);
    }
}
