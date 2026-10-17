//! Images page: local images, their size and which containers use them.

use eframe::egui;

use super::usage_pill;
use crate::app::components::{details, table, widgets};
use crate::app::page::{Page, PageCtx, toggle_selection};
use crate::app::store::Store;
use crate::docker::{ImageInfo, format_size, time_ago};
use crate::worker::Resource;

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

    fn count(&self, store: &Store) -> usize {
        store.images.len()
    }

    fn summary(&self, store: &Store) -> String {
        let total: i64 = store.images.iter().map(|i| i.size).sum();
        format!("{} images · {}", store.images.len(), format_size(total))
    }

    fn search_hint(&self) -> &'static str {
        "Search name or ID"
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
                row.col(|ui| table::two_line(ui, p, image.name(), image.short_id()));
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
            });
    }

    fn clear_selection(&mut self) {
        self.selected = None;
    }
}
