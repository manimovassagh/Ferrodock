//! What the command palette lists: every page, the app's own actions and
//! whatever each page offers, with recently used entries first.

use super::components::palette::Entry;
use super::keys::COMMAND;
use super::page::{CommandRun, Page};
use super::store::Store;

/// How many recently used entries are remembered.
const RECENT: usize = 8;

/// What happens when an entry is picked.
pub enum Target {
    OpenPage(usize),
    /// Something a page offers: run it with that page open.
    Page(usize, CommandRun),
    Refresh,
    Help,
    Settings,
}

pub struct Item {
    pub entry: Entry,
    pub target: Target,
}

/// Every entry, recently used ones (by label) first.
pub fn build(pages: &[Box<dyn Page>], store: &Store, recent: &[String]) -> Vec<Item> {
    let mut items: Vec<Item> = Vec::new();
    for (index, page) in pages.iter().enumerate() {
        items.push(Item {
            entry: Entry {
                label: format!("Go to {}", page.title()),
                detail: format!("{COMMAND}{}", index + 1),
            },
            target: Target::OpenPage(index),
        });
    }
    items.push(item("Refresh", "F5", Target::Refresh));
    items.push(item("Keyboard shortcuts", "?", Target::Help));
    items.push(item("Settings", &format!("{COMMAND},"), Target::Settings));
    for (index, page) in pages.iter().enumerate() {
        items.extend(page.commands(store).into_iter().map(|c| Item {
            entry: Entry {
                label: c.label,
                detail: c.detail,
            },
            target: Target::Page(index, c.run),
        }));
    }
    // Stable sort: recent entries in recency order, then everything else as
    // listed above.
    items.sort_by_key(|i| {
        recent
            .iter()
            .position(|r| *r == i.entry.label)
            .unwrap_or(usize::MAX)
    });
    items
}

fn item(label: &str, detail: &str, target: Target) -> Item {
    Item {
        entry: Entry {
            label: label.into(),
            detail: detail.into(),
        },
        target,
    }
}

/// Moves `label` to the front of the recently used list.
pub fn remember(recent: &mut Vec<String>, label: &str) {
    recent.retain(|r| r != label);
    recent.insert(0, label.to_owned());
    recent.truncate(RECENT);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remembers_most_recent_first() {
        let mut recent = Vec::new();
        for label in ["a", "b", "a"] {
            remember(&mut recent, label);
        }
        assert_eq!(recent, vec!["a", "b"]);
    }
}
