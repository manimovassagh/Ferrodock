//! Volumes page: named and anonymous volumes and which containers mount them.

use eframe::egui;

use super::usage_pill;
use crate::app::components::{details, table, widgets};
use crate::app::page::{Page, PageCtx, toggle_selection};
use crate::app::store::Store;
use crate::docker::{VolumeInfo, time_ago};
use crate::worker::Resource;

#[derive(Default)]
pub struct VolumesPage {
    /// Volumes have no ID; their name is unique.
    selected: Option<String>,
}

impl VolumesPage {
    fn selected<'a>(&self, store: &'a Store) -> Option<&'a VolumeInfo> {
        let name = self.selected.as_deref()?;
        store.volumes.iter().find(|v| v.name == name)
    }
}

impl Page for VolumesPage {
    fn title(&self) -> &'static str {
        "Volumes"
    }

    fn resource(&self) -> Resource {
        Resource::Volumes
    }

    fn count(&self, store: &Store) -> usize {
        store.volumes.len()
    }

    fn summary(&self, store: &Store) -> String {
        let in_use = store
            .volumes
            .iter()
            .filter(|v| !store.containers_using_volume(&v.name).is_empty())
            .count();
        format!("{} volumes · {in_use} in use", store.volumes.len())
    }

    fn search_hint(&self) -> &'static str {
        "Search name or driver"
    }

    fn show(&mut self, ui: &mut egui::Ui, ctx: &mut PageCtx) {
        let p = ctx.palette;
        let store = ctx.store;
        let visible: Vec<&VolumeInfo> = store
            .volumes
            .iter()
            .filter(|v| ctx.matches_search(&[&v.name, &v.driver]))
            .collect();
        if visible.is_empty() {
            let message = if store.volumes.is_empty() {
                "No volumes yet. Create one with `docker volume create` and it will show up here."
            } else {
                "No volumes match."
            };
            widgets::empty_state(ui, p, message);
            return;
        }

        let columns = vec![
            table::Col::fill("NAME"),
            table::Col::fixed("USAGE", 110.0),
            table::Col::fixed("CREATED", 130.0),
        ];
        let selected = self.selected.clone();
        let clicked = table::show(
            ui,
            p,
            "volumes",
            columns,
            visible.len(),
            |i| selected.as_deref() == Some(visible[i].name.as_str()),
            |i, row| {
                let volume = visible[i];
                let users = store.containers_using_volume(&volume.name).len();
                row.col(|ui| table::two_line(ui, p, &volume.name, &volume.driver));
                row.col(|ui| usage_pill(ui, p, users));
                row.col(|ui| {
                    table::muted(ui, p, &volume.created.map(time_ago).unwrap_or_default())
                });
            },
        );
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

        if details::header(ui, p, &volume.name) {
            self.selected = None;
        }
        usage_pill(ui, p, users.len());
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
                    details::field(ui, p, "Containers", &widgets::join_or_dash(&users));
                });
            });
    }

    fn clear_selection(&mut self) {
        self.selected = None;
    }
}
