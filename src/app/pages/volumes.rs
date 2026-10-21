//! Volumes page: named and anonymous volumes and which containers mount them.

use eframe::egui;

use super::create_dialog::{CreateForm, Kind};
use super::files::{self, FilesTab, Target};
use super::marks::{Item, Marks};
use super::{
    confirm_removal, inspect_section, remove_button, remove_task, remove_unused_button, usage_pill,
};
use crate::app::components::icons::Icon;
use crate::app::components::{details, table, widgets};
use crate::app::page::{Command, Intent, Page, PageCtx, toggle_selection};
use crate::app::store::Store;
use crate::docker::{FileOp, Resource, Task, VOLUME_ROOT, VolumeInfo, plural, time_ago};

#[derive(Default)]
pub struct VolumesPage {
    /// Volumes have no ID; their name is unique.
    selected: Option<String>,
    /// The New volume dialog, while it's open.
    create: Option<CreateForm>,
    /// Where the file browser is, while browsing a volume.
    files: FilesTab,
    /// Rows ticked for removing together.
    marks: Marks,
}

/// Stops browsing `volume` and removes its helper container.
fn end_browse(ctx: &mut PageCtx, volume: &str) {
    ctx.request(Intent::Files(FileOp::EndBrowse {
        volume: volume.to_owned(),
    }));
}

impl VolumesPage {
    /// The volume's files, read-only, through a helper container.
    fn browse(&mut self, ui: &mut egui::Ui, ctx: &mut PageCtx, volume: &str) {
        let p = ctx.palette;
        if widgets::button(ui, Icon::Close, "Done browsing").clicked() {
            end_browse(ctx, volume);
            return;
        }
        ui.add_space(10.0);
        let helper = ctx
            .store
            .volume_browser
            .as_ref()
            .and_then(|(_, helper)| helper.clone());
        match helper {
            None => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(egui::RichText::new("Opening the volume…").color(p.muted));
                });
            }
            Some(Err(e)) => {
                ui.label(
                    egui::RichText::new(format!("Couldn't open the volume: {e}")).color(p.danger),
                );
            }
            Some(Ok(container)) => {
                let target = Target {
                    container: &container,
                    root: VOLUME_ROOT,
                    writable: false,
                    note: Some("Read-only: nothing here can be changed from this view."),
                };
                files::show(ui, ctx, &mut self.files, &target);
            }
        }
    }

    fn selected<'a>(&self, store: &'a Store) -> Option<&'a VolumeInfo> {
        let name = self.selected.as_deref()?;
        store.volumes.iter().find(|v| v.name == name)
    }
}

impl Page for VolumesPage {
    fn title(&self) -> &'static str {
        "Volumes"
    }

    fn icon(&self) -> Icon {
        Icon::Database
    }

    fn resource(&self) -> Resource {
        Resource::Volumes
    }

    fn badge(&self, store: &Store) -> String {
        store.volumes.len().to_string()
    }

    fn summary(&self, store: &Store) -> String {
        let in_use = store
            .volumes
            .iter()
            .filter(|v| !store.containers_using_volume(&v.name).is_empty())
            .count();
        format!(
            "{} · {in_use} in use",
            plural(store.volumes.len(), "volume")
        )
    }

    fn search_hint(&self) -> &'static str {
        "Search name or driver"
    }

    fn toolbar(&mut self, ui: &mut egui::Ui, ctx: &mut PageCtx) {
        let unused = unused(ctx.store);
        remove_unused_button(ui, ctx, "volume", unused);
        if widgets::button(ui, Icon::Database, "New volume").clicked() {
            self.create = Some(CreateForm::new(Kind::Volume));
        }
    }

    fn has_overlay(&self) -> bool {
        self.create.is_some()
    }

    fn show(&mut self, ui: &mut egui::Ui, ctx: &mut PageCtx) {
        super::create_dialog(ui, ctx, &mut self.create);
        let p = ctx.palette;
        let store = ctx.store;
        let visible: Vec<&VolumeInfo> = store
            .volumes
            .iter()
            .filter(|v| ctx.matches_search(&[&v.name, &v.driver]))
            .collect();
        if visible.is_empty() {
            super::nothing_to_show(
                ui,
                ctx,
                !store.volumes.is_empty(),
                (
                    Icon::Database,
                    "No volumes",
                    "Volumes appear when containers create them, or with `docker volume create`.",
                ),
                None,
            );
            return;
        }

        let items: Vec<Item> = visible
            .iter()
            .map(|v| Item {
                id: &v.name,
                name: &v.name,
                in_use: !store.containers_using_volume(&v.name).is_empty(),
            })
            .collect();
        self.marks.bar(ui, ctx, "volume", Resource::Volumes, &items);
        let marks = &self.marks;
        let mut ticks = Vec::new();
        // With the details pane open, created goes first; both are in the
        // details pane too.
        let fit = table::fitting(ui, &[28.0], &[110.0, 130.0]);
        let (show_usage, show_created) = (fit[0], fit[1]);
        let mut columns = vec![marks.column(&items), table::Col::fill("NAME")];
        if show_usage {
            columns.push(table::Col::fixed("USAGE", 110.0));
        }
        if show_created {
            columns.push(table::Col::fixed("CREATED", 130.0));
        }
        let selected = self.selected.clone();
        let clicked = table::show(
            ui,
            p,
            "volumes",
            columns,
            visible.len(),
            |i| {
                selected.as_deref() == Some(visible[i].name.as_str()) || marks.contains(items[i].id)
            },
            |i, row| {
                let volume = visible[i];
                let users = store.containers_using_volume(&volume.name);
                let (name, line) = label(volume, &users);
                row.col(|ui| ticks.extend(marks.cell(ui, p, &items[i])));
                row.col(|ui| table::with_icon(ui, p, Icon::Database, p.accent, &name, &line));
                if show_usage {
                    row.col(|ui| usage_pill(ui, p, users.len()));
                }
                if show_created {
                    row.col(|ui| {
                        table::muted(ui, p, &volume.created.map(time_ago).unwrap_or_default())
                    });
                }
            },
        );
        self.marks.apply(ui, "volumes", &items, ticks);
        if let Some(i) = clicked {
            toggle_selection(&mut self.selected, visible[i].name.clone());
        }
    }

    fn has_selection(&self, store: &Store) -> bool {
        self.selected(store).is_some()
    }

    fn show_details(&mut self, ui: &mut egui::Ui, ctx: &mut PageCtx) {
        let p = ctx.palette;
        let Some(volume) = self.selected(ctx.store) else {
            return;
        };
        let users = ctx.store.containers_using_volume(&volume.name);

        // Browsing another volume, or this one in a closed pane: clean up.
        let browsing = match &ctx.store.volume_browser {
            Some((browsed, _)) if *browsed != volume.name => {
                ctx.request(Intent::Files(FileOp::EndBrowse {
                    volume: browsed.clone(),
                }));
                false
            }
            other => other.is_some(),
        };
        if details::header(ui, p, &volume.name) {
            self.selected = None;
            if browsing {
                end_browse(ctx, &volume.name);
            }
        }
        usage_pill(ui, p, users.len());
        ui.add_space(10.0);
        if browsing {
            self.browse(ui, ctx, &volume.name);
            return;
        }
        let blocked = (!users.is_empty()).then_some("Mounted by a container");
        let task = remove_task(Resource::Volumes, &volume.name, &volume.name);
        ui.horizontal(|ui| {
            if widgets::button(ui, Icon::Folder, "Browse files").clicked() {
                ctx.request(Intent::Files(FileOp::BrowseVolume {
                    volume: volume.name.clone(),
                }));
            }
            if !ctx.store.pending.contains_key(&volume.name) {
                remove_button(ui, ctx, "volume", task, blocked);
            } else {
                ui.spinner();
            }
        });
        ui.add_space(16.0);

        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                details::section(ui, p, "Overview", |ui| {
                    details::field(ui, p, "Name", &volume.name);
                    details::field(ui, p, "Driver", &volume.driver);
                    details::field(ui, p, "Scope", &volume.scope);
                    details::field(
                        ui,
                        p,
                        "Created",
                        &volume.created.map(time_ago).unwrap_or_default(),
                    );
                });
                details::section(ui, p, "Location", |ui| {
                    details::field(ui, p, "Mountpoint", &volume.mountpoint);
                });
                details::section(ui, p, "Used by", |ui| {
                    super::container_links(ui, ctx, &users);
                });
                inspect_section(ui, ctx, Resource::Volumes, &volume.name);
            });
    }

    fn clear_selection(&mut self) {
        self.selected = None;
        self.marks.clear();
    }

    fn select_all(&mut self, ctx: &PageCtx) {
        let items: Vec<Item> = ctx
            .store
            .volumes
            .iter()
            .filter(|v| ctx.matches_search(&[&v.name, &v.driver]))
            .map(|v| Item {
                id: &v.name,
                name: &v.name,
                in_use: false,
            })
            .collect();
        self.marks.select_all(&items);
    }

    fn select(&mut self, id: &str) {
        self.selected = Some(id.to_owned());
    }

    fn commands(&self, store: &Store) -> Vec<Command> {
        let mut list: Vec<Command> = store
            .volumes
            .iter()
            .map(|v| Command::select(&v.name, "Volume", &v.name))
            .collect();
        let unused = unused(store);
        if !unused.is_empty() {
            list.push(Command::intent(
                "Remove unused volumes",
                plural(unused.len(), "volume"),
                confirm_removal("volume", unused),
            ));
        }
        list
    }
}

/// Removal tasks for every volume no container mounts.
fn unused(store: &Store) -> Vec<Task> {
    store
        .volumes
        .iter()
        .filter(|v| store.containers_using_volume(&v.name).is_empty())
        .map(|v| remove_task(Resource::Volumes, &v.name, &v.name))
        .collect()
}

/// Whether Docker named the volume itself: a 64-character hex ID, made
/// for a `VOLUME` in an image or a bare `-v /path`.
fn is_anonymous(name: &str) -> bool {
    name.len() == 64 && name.bytes().all(|b| b.is_ascii_hexdigit())
}

/// The name and second line a volume shows in the list. An anonymous
/// volume is named after the container that uses it, with its short ID
/// on the second line.
fn label(volume: &VolumeInfo, users: &[&str]) -> (String, String) {
    if !is_anonymous(&volume.name) {
        return (volume.name.clone(), volume.driver.clone());
    }
    let name = match users.first() {
        Some(user) => format!("anonymous · {user}"),
        None => "anonymous".to_owned(),
    };
    (name, format!("{} · {}", &volume.name[..12], volume.driver))
}

#[cfg(test)]
mod tests {
    use super::is_anonymous;

    #[test]
    fn tells_anonymous_volumes() {
        assert!(is_anonymous(&"0b0e380b057f".repeat(6)[..64]));
        assert!(!is_anonymous("act-toolcache"));
        assert!(!is_anonymous(&"g".repeat(64)));
    }
}
