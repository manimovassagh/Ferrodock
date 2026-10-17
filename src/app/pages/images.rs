//! Images page: local images, their size and which containers use them.

use eframe::egui;

use super::run_dialog::{self, RunForm};
use super::{
    confirm_removal, inspect_section, remove_button, remove_task, remove_unused_button, usage_pill,
};
use crate::app::components::icons::Icon;
use crate::app::components::{details, inspect, table, widgets};
use crate::app::page::{Command, Intent, Page, PageCtx, toggle_selection};
use crate::app::store::Store;
use crate::docker::{ImageInfo, Resource, Task, format_size, plural, time_ago};

#[derive(Default)]
pub struct ImagesPage {
    selected: Option<String>,
    /// What's typed in the Pull box.
    pull_input: String,
    /// The Run dialog, while it's open.
    run_form: Option<RunForm>,
}

impl ImagesPage {
    /// Opens the Run dialog for `image`, loading its exposed ports.
    fn open_run(&mut self, image: &ImageInfo, ctx: &mut PageCtx) {
        let form = RunForm::new(image);
        if inspect::needs_loading(ctx.store, form.image_id()) {
            ctx.request(Intent::Inspect(
                Resource::Images,
                form.image_id().to_owned(),
            ));
        }
        self.run_form = Some(form);
    }

    /// The Pull box and button.
    fn pull_box(&mut self, ui: &mut egui::Ui, ctx: &mut PageCtx) {
        let busy = ctx.store.pending.contains_key(self.pull_input.trim());
        let field = ui.add(
            egui::TextEdit::singleline(&mut self.pull_input)
                .hint_text("Pull an image, e.g. nginx:alpine")
                .desired_width(260.0),
        );
        let submitted = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        let image = self.pull_input.trim().to_owned();
        let clicked = ui
            .add_enabled_ui(!image.is_empty() && !busy, |ui| {
                widgets::button(ui, Icon::Download, "Pull")
            })
            .inner
            .clicked();
        if (clicked || submitted) && !image.is_empty() && !busy {
            ctx.request(Intent::Pull(image));
            self.pull_input.clear();
        }
    }

    fn selected<'a>(&self, store: &'a Store) -> Option<&'a ImageInfo> {
        let id = self.selected.as_deref()?;
        store.images.iter().find(|i| i.id == id)
    }
}

impl Page for ImagesPage {
    fn title(&self) -> &'static str {
        "Images"
    }

    fn icon(&self) -> Icon {
        Icon::Layers
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

    fn has_overlay(&self) -> bool {
        self.run_form.is_some()
    }

    fn show(&mut self, ui: &mut egui::Ui, ctx: &mut PageCtx) {
        let p = ctx.palette;
        let store = ctx.store;
        self.run_dialog(ui, ctx);
        ui.horizontal(|ui| self.pull_box(ui, ctx));
        ui.add_space(8.0);
        pulls(ui, p, store);
        let visible: Vec<&ImageInfo> = store
            .images
            .iter()
            .filter(|i| ctx.matches_search(&[&i.tags.join(" "), i.short_id()]))
            .collect();
        if visible.is_empty() {
            super::nothing_to_show(
                ui,
                ctx,
                !store.images.is_empty(),
                (
                    Icon::Layers,
                    "No images yet",
                    "Pull one with the box above, for example nginx:alpine.",
                ),
                None,
            );
            return;
        }

        let columns = vec![
            table::Col::fill("NAME"),
            table::Col::fixed("USAGE", 110.0),
            table::Col::fixed("SIZE", 100.0),
            table::Col::fixed("CREATED", 130.0),
            table::Col::fixed("", 40.0),
        ];
        let mut run = None;
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
                row.col(|ui| {
                    let line = secondary_line(image);
                    table::with_icon(ui, p, Icon::Layers, p.accent, image.name(), &line)
                });
                row.col(|ui| usage_pill(ui, p, users));
                row.col(|ui| table::text(ui, p, &format_size(image.size)));
                row.col(|ui| table::muted(ui, p, &time_ago(image.created)));
                row.col(|ui| {
                    if (selected.as_deref() == Some(image.id.as_str()) || table::row_hovered(ui))
                        && widgets::icon_button(ui, Icon::Play, "Run as a container").clicked()
                    {
                        run = Some(i);
                    }
                });
            },
        );
        if let Some(i) = run {
            self.open_run(visible[i], ctx);
        } else if let Some(i) = clicked {
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
        let mut run = false;
        ui.horizontal(|ui| {
            run = widgets::primary_button(ui, "Run…").clicked();
            if !ctx.store.pending.contains_key(&image.id) {
                remove_button(ui, ctx, "image", task, blocked);
            } else {
                ui.spinner();
            }
        });
        if run {
            let image = image.clone();
            self.open_run(&image, ctx);
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
                    super::container_links(ui, ctx, &users);
                });
                history_section(ui, ctx, &image.id);
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

impl ImagesPage {
    /// Draws the Run dialog while it's open, and runs the container when
    /// the user confirms.
    fn run_dialog(&mut self, ui: &mut egui::Ui, ctx: &mut PageCtx) {
        let Some(form) = &mut self.run_form else {
            return;
        };
        match run_dialog::show(ui, ctx.palette, form, ctx.store) {
            Some(run_dialog::Outcome::Run(spec)) => {
                ctx.request(Intent::RunImage(spec));
                self.run_form = None;
            }
            Some(run_dialog::Outcome::Cancel) => self.run_form = None,
            None => {}
        }
    }
}

/// Collapsible list of the image's layers, loaded when first opened. The
/// biggest layers stand out, since they're where the size comes from.
fn history_section(ui: &mut egui::Ui, ctx: &mut PageCtx, image_id: &str) {
    let p = ctx.palette;
    egui::CollapsingHeader::new(egui::RichText::new("HISTORY").size(11.0).color(p.muted))
        .id_salt(("history", image_id))
        .show(ui, |ui| {
            let loaded = ctx.store.history.as_ref().filter(|(id, _)| id == image_id);
            let Some((_, Some(layers))) = loaded else {
                if loaded.is_none() {
                    ctx.request(Intent::History(image_id.to_owned()));
                }
                ui.spinner();
                return;
            };
            let mut sizes: Vec<i64> = layers.iter().map(|l| l.size).collect();
            sizes.sort_unstable_by(|a, b| b.cmp(a));
            let big = sizes.get(2).copied().unwrap_or(0).max(1);
            egui::Grid::new(("layers", image_id))
                .num_columns(2)
                .spacing([12.0, 6.0])
                .show(ui, |ui| {
                    for layer in layers {
                        let size = egui::RichText::new(format_size(layer.size)).size(12.0);
                        ui.label(if layer.size >= big {
                            size.color(p.accent).strong()
                        } else {
                            size.color(p.muted)
                        });
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(&layer.instruction)
                                    .monospace()
                                    .size(11.5)
                                    .color(p.text),
                            )
                            .truncate(),
                        )
                        .on_hover_text(&layer.instruction);
                        ui.end_row();
                    }
                });
        });
    ui.add_space(8.0);
}

/// A progress bar for each pull in progress.
fn pulls(ui: &mut egui::Ui, p: &crate::theme::Palette, store: &Store) {
    for (image, progress) in &store.pulls {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(format!("Pulling {image}")).color(p.text));
            let detail = if progress.total > 0 {
                format!(
                    "{} · {} of {}",
                    progress.status,
                    format_size(progress.done as i64),
                    format_size(progress.total as i64)
                )
            } else {
                progress.status.clone()
            };
            ui.label(egui::RichText::new(detail).color(p.muted));
        });
        let fraction = if progress.total > 0 {
            progress.done as f32 / progress.total as f32
        } else {
            0.0
        };
        ui.add(
            egui::ProgressBar::new(fraction)
                .desired_height(6.0)
                .fill(p.accent),
        );
        ui.add_space(8.0);
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
