//! Images page: local images, their size and which containers use them;
//! pulling, Docker Hub search, tagging, pushing, and saving and loading
//! `.tar` files.

mod search;
mod tag_dialog;

use eframe::egui;

use super::marks::{Item, Marks};
use super::run_dialog::{self, RunForm};
use super::{
    confirm_removal, inspect_section, remove_button, remove_task, remove_unused_button, usage_pill,
};
use crate::app::components::icons::Icon;
use crate::app::components::{details, inspect, table, widgets};
use crate::app::page::{Command, Intent, Page, PageCtx, toggle_selection};
use crate::app::store::Store;
use crate::docker::{ImageInfo, ImageOp, Resource, Task, format_size, plural, time_ago};
use tag_dialog::TagForm;

#[derive(Default)]
pub struct ImagesPage {
    selected: Option<String>,
    /// What's typed in the Pull box.
    pull_input: String,
    /// The Run dialog, while it's open.
    run_form: Option<RunForm>,
    /// The Tag dialog, while it's open.
    tag_form: Option<TagForm>,
    /// Whether the search results show.
    searching: bool,
    /// When the Pull box last changed, to search once typing pauses.
    typed_at: Option<std::time::Instant>,
    /// Rows ticked for removing together.
    marks: Marks,
}

/// How long typing has to pause before Docker Hub is searched.
const SEARCH_DELAY: std::time::Duration = std::time::Duration::from_millis(400);

/// What to search Docker Hub for, given what's typed in the Pull box: the
/// name without a tag (`nginx:alpine` → `nginx`), since the search doesn't
/// know tags. None when it's too short to be worth it.
fn search_term(input: &str) -> Option<&str> {
    let input = input.trim();
    let name = match input.rsplit_once(':') {
        Some((name, tag)) if !tag.contains('/') => name,
        _ => input,
    };
    (name.chars().count() >= 2).then_some(name)
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
                .hint_text("Search Docker Hub or pull an image, e.g. nginx:alpine")
                // Narrower with the details pane open, leaving room for
                // the Pull button.
                .desired_width((ui.available_width() - 90.0).clamp(160.0, 380.0)),
        );
        if field.changed() {
            self.typed_at = Some(std::time::Instant::now());
        }
        let submitted = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        let image = self.pull_input.trim().to_owned();
        let pull = ui
            .add_enabled_ui(!image.is_empty() && !busy, |ui| {
                widgets::button(ui, Icon::Download, "Pull")
            })
            .inner;
        // The button names the field for screen readers.
        field.labelled_by(pull.id);
        let clicked = pull.clicked();
        if (clicked || submitted) && !image.is_empty() && !busy {
            ctx.request(Intent::Pull(image.clone()));
            self.pull_input.clear();
            self.searching = false;
            self.typed_at = None;
            return;
        }
        self.search_when_paused(ui, ctx);
    }

    /// Searches Docker Hub once typing in the Pull box has paused, and
    /// hides the results when the box is emptied.
    fn search_when_paused(&mut self, ui: &egui::Ui, ctx: &mut PageCtx) {
        let Some(typed_at) = self.typed_at else {
            return;
        };
        let Some(term) = search_term(&self.pull_input) else {
            self.searching = false;
            self.typed_at = None;
            return;
        };
        let waited = typed_at.elapsed();
        if waited < SEARCH_DELAY {
            ui.ctx().request_repaint_after(SEARCH_DELAY - waited);
            return;
        }
        self.typed_at = None;
        let already = ctx
            .store
            .search
            .as_ref()
            .is_some_and(|(asked, _)| asked == term);
        if !already {
            ctx.request(Intent::Image(ImageOp::Search(term.to_owned())));
        }
        self.searching = true;
    }

    /// Loads `.tar` files dropped on the window; says where they'll go
    /// while they're dragged over it.
    fn loads(&self, ui: &mut egui::Ui, ctx: &mut PageCtx) {
        let p = ctx.palette;
        let (hovering, dropped) = ui.input(|i| {
            let dropped: Vec<_> = i
                .raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("tar")))
                .collect();
            (!i.raw.hovered_files.is_empty(), dropped)
        });
        for path in dropped {
            ctx.request(Intent::Image(ImageOp::Load(path)));
        }
        if hovering {
            egui::Frame::new()
                .fill(p.accent.gamma_multiply(0.15))
                .stroke(egui::Stroke::new(1.5, p.accent))
                .corner_radius(egui::CornerRadius::same(8))
                .inner_margin(egui::Margin::same(12))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(
                        egui::RichText::new("Drop an image .tar to load it")
                            .strong()
                            .color(p.accent),
                    );
                });
            ui.add_space(8.0);
        }
    }

    /// The Tag dialog, while it's open.
    fn tag_dialog(&mut self, ui: &mut egui::Ui, ctx: &mut PageCtx) {
        let Some(form) = &mut self.tag_form else {
            return;
        };
        match tag_dialog::show(ui, ctx.palette, form) {
            Some(Some(target)) => {
                ctx.request(Intent::Image(ImageOp::Tag {
                    image: form.image_id.clone(),
                    target,
                }));
                self.tag_form = None;
            }
            Some(None) => self.tag_form = None,
            None => {}
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
        self.run_form.is_some() || self.tag_form.is_some()
    }

    fn show(&mut self, ui: &mut egui::Ui, ctx: &mut PageCtx) {
        let p = ctx.palette;
        let store = ctx.store;
        self.run_dialog(ui, ctx);
        self.tag_dialog(ui, ctx);
        self.loads(ui, ctx);
        ui.horizontal(|ui| self.pull_box(ui, ctx));
        ui.add_space(8.0);
        if self.searching {
            let mut run = None;
            self.searching = search::show(ui, ctx, &mut run);
            if let Some(reference) = run {
                self.run_form = Some(RunForm::for_reference(&reference));
            }
        }
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

        let items: Vec<Item> = visible
            .iter()
            .map(|i| Item {
                id: &i.id,
                name: i.name(),
                in_use: !store.containers_using_image(&i.id).is_empty(),
            })
            .collect();
        self.marks.bar(ui, ctx, "image", Resource::Images, &items);
        let marks = &self.marks;
        let mut ticks = Vec::new();
        // With the details pane open, size goes last, then usage and
        // created; all three are in the details pane too.
        let fit = table::fitting(ui, &[28.0, 40.0], &[100.0, 110.0, 130.0]);
        let (show_size, show_usage, show_created) = (fit[0], fit[1], fit[2]);
        let mut columns = vec![marks.column(&items), table::Col::fill("NAME")];
        if show_usage {
            columns.push(table::Col::fixed("USAGE", 110.0));
        }
        if show_size {
            columns.push(table::Col::fixed("SIZE", 100.0));
        }
        if show_created {
            columns.push(table::Col::fixed("CREATED", 130.0));
        }
        columns.push(table::Col::fixed("", 40.0));
        let mut run = None;
        let selected = self.selected.clone();
        let clicked = table::show(
            ui,
            p,
            "images",
            columns,
            visible.len(),
            |i| selected.as_deref() == Some(visible[i].id.as_str()) || marks.contains(items[i].id),
            |i, row| {
                let image = visible[i];
                let users = store.containers_using_image(&image.id).len();
                row.col(|ui| ticks.extend(marks.cell(ui, p, &items[i])));
                row.col(|ui| {
                    let line = secondary_line(image);
                    table::with_icon(ui, p, Icon::Layers, p.accent, image.name(), &line)
                });
                if show_usage {
                    row.col(|ui| usage_pill(ui, p, users));
                }
                if show_size {
                    row.col(|ui| table::text(ui, p, &format_size(image.size)));
                }
                if show_created {
                    row.col(|ui| table::muted(ui, p, &time_ago(image.created)));
                }
                row.col(|ui| {
                    if (selected.as_deref() == Some(image.id.as_str()) || table::row_hovered(ui))
                        && widgets::icon_button(ui, Icon::Play, "Run as a container").clicked()
                    {
                        run = Some(i);
                    }
                });
            },
        );
        self.marks.apply(ui, "images", &items, ticks);
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
            let more = widgets::icon_button(ui, Icon::More, "More actions");
            egui::Popup::menu(&more).show(|ui| self.more_menu(ui, ctx, image));
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
        self.marks.clear();
    }

    fn select_all(&mut self, ctx: &PageCtx) {
        let items: Vec<Item> = ctx
            .store
            .images
            .iter()
            .filter(|i| ctx.matches_search(&[&i.tags.join(" "), i.short_id()]))
            .map(|i| Item {
                id: &i.id,
                name: i.name(),
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
    /// The ⋯ menu: tag, push and save.
    fn more_menu(&mut self, ui: &mut egui::Ui, ctx: &mut PageCtx, image: &ImageInfo) {
        ui.set_min_width(170.0);
        if ui.button("Tag…").clicked() {
            self.tag_form = Some(TagForm {
                image_id: image.id.clone(),
                current: image.name().to_owned(),
                target: String::new(),
            });
        }
        let name = image.tags.first();
        let push = ui
            .add_enabled(name.is_some(), egui::Button::new("Push"))
            .on_hover_text(format!(
                "Push {} with your Docker login",
                name.map_or("", String::as_str)
            ))
            .on_disabled_hover_text("Tag it with a registry name first");
        if push.clicked()
            && let Some(name) = name
        {
            ctx.request(Intent::Image(ImageOp::Push(name.clone())));
        }
        let save = ui
            .button("Save as .tar")
            .on_hover_text("Save it to your Downloads folder, to move it without a registry");
        if save.clicked() {
            let reference = name.cloned().unwrap_or_else(|| image.id.clone());
            ctx.request(Intent::Image(ImageOp::Save(reference)));
        }
    }

    /// Draws the Run dialog while it's open, and runs the container when
    /// the user confirms.
    fn run_dialog(&mut self, ui: &mut egui::Ui, ctx: &mut PageCtx) {
        run_dialog::show_open(ui, ctx, &mut self.run_form);
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
            let verb = if progress.pushing {
                "Pushing"
            } else {
                "Pulling"
            };
            ui.label(egui::RichText::new(format!("{verb} {image}")).color(p.text));
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

#[cfg(test)]
mod tests {
    use super::search_term;

    #[test]
    fn searches_for_the_name_without_its_tag() {
        assert_eq!(search_term("nginx:alpine"), Some("nginx"));
        assert_eq!(search_term(" postgres "), Some("postgres"));
        assert_eq!(
            search_term("localhost:5000/app"),
            Some("localhost:5000/app")
        );
        assert_eq!(search_term("r"), None);
        assert_eq!(search_term(""), None);
    }
}
