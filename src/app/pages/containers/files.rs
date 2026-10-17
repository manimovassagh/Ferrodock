//! Files tab: browse the container's filesystem, preview text files,
//! download anything to the Downloads folder, and drop files in to upload.

use eframe::egui::{self, CornerRadius, RichText, Sense, Stroke, vec2};

use crate::app::components::icons::{self, Icon};
use crate::app::components::widgets;
use crate::app::page::{Intent, PageCtx};
use crate::docker::{
    ContainerInfo, FileEntry, FileKind, FileOp, Preview, format_size, join_path, parent_path,
};
use crate::theme::Palette;

/// Where the user is in a container's filesystem.
#[derive(Default)]
pub(super) struct FilesTab {
    container: String,
    path: String,
    /// The file being previewed.
    selected: Option<String>,
    /// A link opened as a folder. If listing it fails, it's shown as a
    /// file instead.
    via_link: Option<String>,
}

impl FilesTab {
    /// Starts at `/` whenever another container is shown.
    fn follow(&mut self, container: &str) {
        if self.container != container {
            *self = Self {
                container: container.to_owned(),
                path: "/".into(),
                selected: None,
                via_link: None,
            };
        }
    }

    fn open(&mut self, path: String) {
        self.path = path;
        self.selected = None;
        self.via_link = None;
    }

    /// Previews the file at `path`, which is in the open folder.
    fn select(&mut self, ctx: &mut PageCtx, path: String) {
        ctx.request(Intent::Files(FileOp::Preview {
            container: self.container.clone(),
            path: path.clone(),
        }));
        self.selected = Some(path);
    }

    fn op_list(&self) -> FileOp {
        FileOp::List {
            container: self.container.clone(),
            path: self.path.clone(),
        }
    }
}

pub(super) fn show(ui: &mut egui::Ui, ctx: &mut PageCtx, tab: &mut FilesTab, c: &ContainerInfo) {
    let p = ctx.palette;
    tab.follow(&c.id);
    let view = ctx.store.files.as_ref().filter(|v| v.container_id == c.id);
    if view.is_none_or(|v| !v.shows(&c.id, &tab.path)) {
        ctx.request(Intent::Files(tab.op_list()));
    }
    uploads(ui, ctx, tab);

    toolbar(ui, ctx, tab, view.is_some_and(|v| v.transfers > 0));
    ui.add_space(8.0);

    let listing = view.and_then(|v| v.listing.as_ref());
    let Some(listing) = listing else {
        ui.add_space(20.0);
        ui.vertical_centered(|ui| ui.spinner());
        return;
    };
    let listing = match listing {
        Ok(listing) => listing,
        Err(_) if tab.via_link.as_deref() == Some(tab.path.as_str()) => {
            // A link to a file, not a folder: go back and preview it.
            let file = tab.path.clone();
            tab.open(parent_path(&file));
            tab.select(ctx, file);
            return;
        }
        Err(message) => {
            ui.label(RichText::new(message).color(p.danger));
            return;
        }
    };
    if !c.is_running() {
        ui.label(
            RichText::new("The container is stopped: this is its filesystem as it was left.")
                .size(12.0)
                .color(p.muted),
        );
        ui.add_space(4.0);
    }
    if !listing.complete {
        ui.label(
            RichText::new("This folder is too big to list completely.")
                .size(12.0)
                .color(p.warning),
        );
    }

    let preview = tab.selected.is_some();
    let list_height = if preview {
        (ui.available_height() * 0.45).max(120.0)
    } else {
        ui.available_height() - 24.0
    };
    egui::Frame::new()
        .fill(p.bg)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(egui::Margin::same(4))
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt(("files", &tab.path))
                .max_height(list_height)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if tab.path != "/" && row(ui, p, None, false).clicked() {
                        tab.open(parent_path(&tab.path));
                    }
                    if listing.entries.is_empty() {
                        ui.label(RichText::new("  Empty folder").color(p.muted));
                    }
                    for entry in &listing.entries {
                        let path = join_path(&tab.path, &entry.name);
                        let selected = tab.selected.as_deref() == Some(path.as_str());
                        if row(ui, p, Some(entry), selected).clicked() {
                            match entry.kind {
                                FileKind::Dir => tab.open(path),
                                FileKind::Link => {
                                    tab.open(path.clone());
                                    tab.via_link = Some(path);
                                }
                                FileKind::File => tab.select(ctx, path),
                                FileKind::Other => {}
                            }
                        }
                    }
                });
        });

    if let Some(path) = tab.selected.clone() {
        ui.add_space(10.0);
        preview_pane(ui, ctx, tab, &path);
    } else {
        ui.add_space(6.0);
        ui.label(
            RichText::new("Drop files here to copy them into this folder.")
                .size(12.0)
                .color(p.muted),
        );
    }
}

/// Breadcrumbs for the current folder, with Refresh and Download.
fn toolbar(ui: &mut egui::Ui, ctx: &mut PageCtx, tab: &mut FilesTab, busy: bool) {
    let p = ctx.palette;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        let mut target = None;
        if crumb(ui, p, "/", tab.path == "/").clicked() {
            target = Some("/".to_owned());
        }
        let parts: Vec<&str> = tab.path.split('/').filter(|s| !s.is_empty()).collect();
        for (i, part) in parts.iter().enumerate() {
            if i > 0 {
                ui.label(RichText::new("/").color(p.muted));
            }
            let path = format!("/{}", parts[..=i].join("/"));
            if crumb(ui, p, part, i + 1 == parts.len()).clicked() {
                target = Some(path);
            }
        }
        if let Some(path) = target {
            tab.open(path);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if widgets::icon_button(ui, Icon::Refresh, "List again").clicked() {
                ctx.request(Intent::Files(tab.op_list()));
            }
            let download = widgets::icon_button(
                ui,
                Icon::Download,
                "Download this folder to your Downloads folder",
            );
            if download.clicked() {
                ctx.request(Intent::Files(FileOp::Download {
                    container: tab.container.clone(),
                    path: tab.path.clone(),
                }));
            }
            if busy {
                ui.spinner();
                ui.label(RichText::new("Copying…").size(12.0).color(p.muted));
            }
        });
    });
}

fn crumb(ui: &mut egui::Ui, p: &Palette, label: &str, current: bool) -> egui::Response {
    let color = if current { p.text } else { p.accent };
    ui.add(egui::Label::new(RichText::new(label).monospace().color(color)).sense(Sense::click()))
        .on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// One row of the listing; `None` is the ".." row.
fn row(
    ui: &mut egui::Ui,
    p: &Palette,
    entry: Option<&FileEntry>,
    selected: bool,
) -> egui::Response {
    let height = 26.0;
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::click());
    let painter = ui.painter_at(rect);
    if selected {
        painter.rect_filled(rect, CornerRadius::same(6), p.accent.gamma_multiply(0.18));
    } else if response.hovered() {
        painter.rect_filled(rect, CornerRadius::same(6), p.raised);
    }
    let (icon, name, color) = match entry {
        None => (Icon::Folder, "..".to_owned(), p.muted),
        Some(e) => match e.kind {
            FileKind::Dir => (Icon::Folder, e.name.clone(), p.text),
            FileKind::Link => (
                Icon::File,
                format!("{} → {}", e.name, e.target.as_deref().unwrap_or("?")),
                p.text,
            ),
            FileKind::File => (Icon::File, e.name.clone(), p.text),
            FileKind::Other => (Icon::File, e.name.clone(), p.muted),
        },
    };
    let icon_color = match entry.map(|e| e.kind) {
        Some(FileKind::Dir) | None => p.accent,
        _ => p.muted,
    };
    icons::paint(
        &painter,
        egui::Rect::from_center_size(rect.left_center() + vec2(16.0, 0.0), vec2(15.0, 15.0)),
        icon,
        icon_color,
    );
    let font = egui::FontId::proportional(13.0);
    let meta_font = egui::FontId::proportional(12.0);
    // Size and date on the right, the name gets what's left.
    let meta = entry
        .map(|e| {
            let size = match e.kind {
                FileKind::File => format_size(e.size as i64),
                _ => String::new(),
            };
            format!("{size:>9}   {}", e.modified)
        })
        .unwrap_or_default();
    let meta_galley = painter.layout_no_wrap(meta, meta_font, p.muted);
    let meta_pos =
        rect.right_center() - vec2(meta_galley.size().x + 10.0, meta_galley.size().y / 2.0);
    let name_width = (meta_pos.x - rect.left() - 44.0).max(40.0);
    let mut job =
        egui::text::LayoutJob::single_section(name, egui::TextFormat::simple(font, color));
    job.wrap = egui::text::TextWrapping::truncate_at_width(name_width);
    let name_galley = painter.layout_job(job);
    painter.galley(
        rect.left_center() + vec2(32.0, -name_galley.size().y / 2.0),
        name_galley,
        color,
    );
    painter.galley(meta_pos, meta_galley, p.muted);
    let label = entry.map_or("..", |e| e.name.as_str()).to_owned();
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &label));
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// The selected file: its contents (if it's small text) and a Download
/// button.
fn preview_pane(ui: &mut egui::Ui, ctx: &mut PageCtx, tab: &mut FilesTab, path: &str) {
    let p = ctx.palette;
    let name = path.rsplit('/').next().unwrap_or(path);
    ui.horizontal(|ui| {
        ui.label(RichText::new(name).strong().color(p.text));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if widgets::icon_button(ui, Icon::Close, "Close the preview").clicked() {
                tab.selected = None;
            }
            if widgets::button(ui, Icon::Download, "Download").clicked() {
                ctx.request(Intent::Files(FileOp::Download {
                    container: tab.container.clone(),
                    path: path.to_owned(),
                }));
            }
        });
    });
    ui.add_space(4.0);
    let preview = ctx
        .store
        .files
        .as_ref()
        .and_then(|v| v.preview.as_ref())
        .filter(|(asked, _)| asked == path)
        .and_then(|(_, result)| result.as_ref());
    let message = |ui: &mut egui::Ui, text: &str| {
        ui.label(RichText::new(text).color(p.muted));
    };
    match preview {
        None => {
            ui.spinner();
        }
        Some(Err(e)) => {
            ui.label(RichText::new(e).color(p.danger));
        }
        Some(Ok(Preview::Binary)) => message(ui, "A binary file: download it to open it."),
        Some(Ok(Preview::TooLarge(size))) => message(
            ui,
            &format!(
                "{} is too big to preview: download it to open it.",
                format_size(*size as i64)
            ),
        ),
        Some(Ok(Preview::Text(text))) if text.is_empty() => message(ui, "Empty file."),
        Some(Ok(Preview::Text(text))) => {
            egui::Frame::new()
                .fill(p.bg)
                .stroke(Stroke::new(1.0, p.border))
                .corner_radius(CornerRadius::same(8))
                .inner_margin(egui::Margin::same(10))
                .show(ui, |ui| {
                    egui::ScrollArea::both()
                        .id_salt(("preview", path))
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.add(
                                egui::Label::new(
                                    RichText::new(text).monospace().size(12.0).color(p.text),
                                )
                                .extend(),
                            );
                        });
                });
        }
    }
}

/// Files dropped on the window go into the open folder. While files are
/// dragged over it, a banner says where they'll go.
fn uploads(ui: &mut egui::Ui, ctx: &mut PageCtx, tab: &FilesTab) {
    let p = ctx.palette;
    let (hovering, dropped) = ui.input(|i| {
        let dropped: Vec<_> = i
            .raw
            .dropped_files
            .iter()
            .map(|f| f.path().to_path_buf())
            .filter(|p| !p.as_os_str().is_empty())
            .collect();
        (!i.raw.hovered_files.is_empty(), dropped)
    });
    if !dropped.is_empty() {
        ctx.request(Intent::Files(FileOp::Upload {
            container: tab.container.clone(),
            dir: tab.path.clone(),
            files: dropped,
        }));
    }
    if hovering {
        egui::Frame::new()
            .fill(p.accent.gamma_multiply(0.15))
            .stroke(Stroke::new(1.5, p.accent))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(egui::Margin::same(12))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(
                    RichText::new(format!("Drop to copy into {}", tab.path))
                        .strong()
                        .color(p.accent),
                );
            });
        ui.add_space(8.0);
    }
}
