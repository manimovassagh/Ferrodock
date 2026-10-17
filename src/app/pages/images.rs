//! Images page: local images, their size and which containers use them.

use eframe::egui;

use super::{
    confirm_removal, inspect_section, remove_button, remove_task, remove_unused_button, usage_pill,
};
use crate::app::components::{details, table, widgets};
use crate::app::page::{Command, Page, PageCtx, toggle_selection};
use crate::app::store::Store;
use crate::docker::{ImageInfo, Resource, Task, format_size, plural, time_ago};

#[derive(Default)]
pub struct ImagesPage {
    selected: Option<String>,
}

impl ImagesPage {
    fn selected<'a>(&self, store: &'a Store) -> Option<&'a ImageInfo> {
        let id = self.selected.as_deref()?;
        store.images.iter().find(|i| i.id == id)
    }
}

impl Page for ImagesPage {
    fn title(&self) -> &'static str {
        "Images"
    }

    fn resource(&self) -> Resource {
        Resource::Images
    }

    fn badge(&self, store: &Store) -> String {
        store.images.len().to_string()
    }

    fn summary(&self, store: &Store) -> String {
        let total: i64 = store.images.iter().map(|i| i.size).sum();
        format!(
            "{} · {}",
            plural(store.images.len(), "image"),
            format_size(total)
        )
    }

    fn search_hint(&self) -> &'static str {
        "Search name or ID"
    }

    fn toolbar(&mut self, ui: &mut egui::Ui, ctx: &mut PageCtx) {
        remove_unused_button(ui, ctx, "image", unused(ctx.store));
    }

    fn show(&mut self, ui: &mut egui::Ui, ctx: &mut PageCtx) {
        let p = ctx.palette;
        let store = ctx.store;
        let visible: Vec<&ImageInfo> = store
            .images
            .iter()
            .filter(|i| ctx.matches_search(&[&i.tags.join(" "), i.short_id()]))
            .collect();
        if visible.is_empty() {
            let message = if store.images.is_empty() {
                "No images yet. Pull one with `docker pull` and it will show up here."
            } else {
                "No images match."
            };
            widgets::empty_state(ui, p, message);
            return;
        }

        let columns = vec![
            table::Col::fill("NAME"),
            table::Col::fixed("USAGE", 110.0),
            table::Col::fixed("SIZE", 100.0),
            table::Col::fixed("CREATED", 130.0),
        ];
        let selected = self.selected.clone();
        let clicked = table::show(
            ui,
            p,
            "images",
            columns,
            visible.len(),
            |i| selected.as_deref() == Some(visible[i].id.as_str()),
            |i, row| {
                let image = visible[i];
                let users = store.containers_using_image(&image.id).len();
                row.col(|ui| table::two_line(ui, p, image.name(), &secondary_line(image)));
                row.col(|ui| usage_pill(ui, p, users));
                row.col(|ui| table::text(ui, p, &format_size(image.size)));
                row.col(|ui| table::muted(ui, p, &time_ago(image.created)));
            },
        );
        if let Some(i) = clicked {
            toggle_selection(&mut self.selected, visible[i].id.clone());
        }
    }

    fn has_selection(&self, store: &Store) -> bool {
        self.selected(store).is_some()
    }

    fn show_details(&mut self, ui: &mut egui::Ui, ctx: &mut PageCtx) {
        let p = ctx.palette;
        let Some(image) = self.selected(ctx.store) else {
            return;
        };
        let users = ctx.store.containers_using_image(&image.id);

        if details::header(ui, p, image.name()) {
            self.selected = None;
        }
        usage_pill(ui, p, users.len());
        ui.add_space(10.0);
        let blocked = (!users.is_empty()).then_some("Used by a container");
        let task = remove_task(Resource::Images, &image.id, image.name());
        if !ctx.store.pending.contains(&image.id) {
            remove_button(ui, ctx, "image", task, blocked);
        } else {
            ui.spinner();
        }
        ui.add_space(16.0);

        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                details::section(ui, p, "Overview", |ui| {
                    details::field(ui, p, "Tags", &widgets::join_or_dash(&image.tags));
                    details::copy_field(ui, p, "ID", image.short_id(), &image.id);
                    details::field(ui, p, "Size", &format_size(image.size));
                    details::field(ui, p, "Created", &time_ago(image.created));
                });
                details::section(ui, p, "Used by", |ui| {
                    details::field(ui, p, "Containers", &widgets::join_or_dash(&users));
                });
                inspect_section(ui, ctx, Resource::Images, &image.id);
            });
    }

    fn clear_selection(&mut self) {
        self.selected = None;
    }

    fn select(&mut self, id: &str) {
        self.selected = Some(id.to_owned());
    }

    fn commands(&self, store: &Store) -> Vec<Command> {
        let mut list: Vec<Command> = store
            .images
            .iter()
            .map(|i| Command::select(i.name(), format!("Image · {}", format_size(i.size)), &i.id))
            .collect();
        let unused = unused(store);
        if !unused.is_empty() {
            list.push(Command::intent(
                "Remove unused images",
                plural(unused.len(), "image"),
                confirm_removal("image", unused),
            ));
        }
        list
    }
}

/// Short ID, plus how many more tags the image has beyond the one shown.
fn secondary_line(image: &ImageInfo) -> String {
    match image.tags.len() {
        0 | 1 => image.short_id().to_owned(),
        n => format!("{} · +{}", image.short_id(), plural(n - 1, "tag")),
    }
}

/// Removal tasks for every image no container uses.
fn unused(store: &Store) -> Vec<Task> {
    store
        .images
        .iter()
        .filter(|i| store.containers_using_image(&i.id).is_empty())
        .map(|i| remove_task(Resource::Images, &i.id, i.name()))
        .collect()
}
