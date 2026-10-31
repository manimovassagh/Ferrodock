//! Kubernetes page: local clusters, made only when asked (k3s in
//! containers). Clusters are listed with their state; an open cluster shows
//! its nodes and pods, a shell with kubectl, and loads local images into it.

mod form;

use std::collections::HashSet;
use std::time::{Duration, Instant};

use eframe::egui::{self, RichText};

use super::containers::terminal::{self, TerminalTab};
use crate::app::components::icons::Icon;
use crate::app::components::widgets;
use crate::app::page::{Command, Confirmation, Intent, Page, PageCtx};
use crate::app::store::Store;
use crate::docker::{
    ClusterStatus, ContainerInfo, KUBE_VERSIONS, KubeLabels, KubeOp, KubePod, Resource,
};
use crate::theme::Palette;
use form::ClusterForm;

/// How often an open cluster's nodes and pods are read again.
const STATUS_EVERY: Duration = Duration::from_secs(3);

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum Tab {
    #[default]
    Overview,
    Terminal,
    LoadImage,
}

#[derive(Default)]
pub struct KubePage {
    /// The Create dialog, while it's open.
    form: Option<ClusterForm>,
    /// The cluster shown across the page.
    open: Option<String>,
    /// A cluster just asked for, opened once it's up.
    awaited: Option<String>,
    tab: Tab,
    terminal: TerminalTab,
    /// The image picked on the Load image tab.
    image: String,
    /// Failed creations the user closed, by name.
    dismissed: HashSet<String>,
    /// When the open cluster's status was last asked for.
    asked: Option<Instant>,
}

/// A cluster, from its containers.
struct Cluster<'a> {
    name: &'a str,
    labels: &'a KubeLabels,
    server: Option<&'a ContainerInfo>,
    workers: Vec<&'a ContainerInfo>,
}

impl Cluster<'_> {
    fn running(&self) -> bool {
        self.server.is_some_and(ContainerInfo::is_running)
    }

    /// `1.37`, from the server's image tag.
    fn version(&self) -> String {
        let tag = self
            .server
            .and_then(|s| s.image.rsplit_once(':'))
            .map_or("", |(_, tag)| tag);
        KUBE_VERSIONS.iter().find(|(_, t)| *t == tag).map_or_else(
            || tag.trim_start_matches('v').to_owned(),
            |(v, _)| (*v).to_owned(),
        )
    }

    fn nodes_text(&self) -> String {
        match self.workers.len() {
            0 => "1 node".to_owned(),
            1 => "1 server, 1 worker".to_owned(),
            n => format!("1 server, {n} workers"),
        }
    }
}

fn clusters(store: &Store) -> Vec<Cluster<'_>> {
    let mut list: Vec<Cluster> = Vec::new();
    for c in &store.containers {
        let Some(labels) = &c.kube else { continue };
        let i = match list.iter().position(|k| k.name == labels.cluster) {
            Some(i) => i,
            None => {
                list.push(Cluster {
                    name: &labels.cluster,
                    labels,
                    server: None,
                    workers: Vec::new(),
                });
                list.len() - 1
            }
        };
        if labels.server {
            list[i].server = Some(c);
            list[i].labels = labels;
        } else {
            list[i].workers.push(c);
        }
    }
    list.sort_by(|a, b| a.name.cmp(b.name));
    for k in &mut list {
        k.workers.sort_by(|a, b| a.name.cmp(&b.name));
    }
    list
}

fn busy(store: &Store, cluster: &str) -> bool {
    store.pending.contains_key(&format!("kube:{cluster}"))
}

fn section(ui: &mut egui::Ui, p: &Palette, title: &str) {
    ui.label(
        RichText::new(title)
            .font(egui::FontId::new(11.5, crate::theme::semibold()))
            .color(p.muted),
    );
    ui.add_space(6.0);
}

fn row_frame(ui: &mut egui::Ui, p: &Palette, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(p.bg)
        .stroke(egui::Stroke::new(1.0, p.border))
        .corner_radius(egui::CornerRadius::same(10))
        .inner_margin(egui::Margin::symmetric(12, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(add);
        });
    ui.add_space(8.0);
}

/// Asks, then removes the cluster and its kubectl context.
fn confirm_remove(k: &Cluster) -> Intent {
    let mut message =
        "Its nodes and everything running in it are deleted. This can't be undone.".to_owned();
    if !k.labels.context.is_empty() {
        message.push_str(&format!(
            " The kubectl context {} is removed too.",
            k.labels.context
        ));
    }
    Intent::Confirm(Confirmation {
        title: format!("Remove the cluster “{}”?", k.name),
        message,
        action_label: "Remove".into(),
        tasks: Vec::new(),
        intents: vec![Intent::Kube(KubeOp::Remove {
            cluster: k.name.to_owned(),
            context: k.labels.context.clone(),
        })],
        ..Default::default()
    })
}

/// The colour for a pod's status.
fn pod_color(p: &Palette, pod: &KubePod) -> egui::Color32 {
    if pod.healthy() {
        p.success
    } else if matches!(
        pod.status.as_str(),
        "Pending" | "ContainerCreating" | "PodInitializing" | "Terminating"
    ) {
        p.warning
    } else {
        p.danger
    }
}

impl Page for KubePage {
    fn title(&self) -> &'static str {
        "Kubernetes"
    }

    fn icon(&self) -> Icon {
        Icon::Wheel
    }

    fn resource(&self) -> Resource {
        Resource::Containers
    }

    fn badge(&self, store: &Store) -> String {
        match clusters(store).iter().filter(|k| k.running()).count() {
            0 => String::new(),
            n => n.to_string(),
        }
    }

    fn summary(&self, store: &Store) -> String {
        let all = clusters(store);
        if all.is_empty() {
            return "Local clusters, only when you want one".into();
        }
        let running = all.iter().filter(|k| k.running()).count();
        format!(
            "{} · {running} running",
            crate::docker::plural(all.len(), "cluster")
        )
    }

    fn search_hint(&self) -> &'static str {
        "Filter clusters"
    }

    fn bottom(&self) -> bool {
        true
    }

    fn has_overlay(&self) -> bool {
        self.form.is_some()
    }

    fn toolbar(&mut self, ui: &mut egui::Ui, ctx: &mut PageCtx) {
        if self.open.is_none()
            && !clusters(ctx.store).is_empty()
            && widgets::button(ui, Icon::Wheel, "New cluster").clicked()
        {
            self.form = Some(ClusterForm::new(ctx.store));
        }
    }

    fn show(&mut self, ui: &mut egui::Ui, ctx: &mut PageCtx) {
        let p = ctx.palette;
        if let Some(open) = &mut self.form {
            match form::show(ui, p, ctx.store, open) {
                Some(form::Outcome::Create(spec)) => {
                    self.dismissed.remove(&spec.name);
                    self.awaited = Some(spec.name.clone());
                    ctx.request(Intent::Kube(KubeOp::Create(spec)));
                    self.form = None;
                }
                Some(form::Outcome::Cancel) => self.form = None,
                None => {}
            }
        }
        // A cluster just created opens once it's up.
        if let Some(name) = &self.awaited {
            if ctx.store.kube_errors.contains_key(name) {
                self.awaited = None;
            } else if !busy(ctx.store, name)
                && clusters(ctx.store)
                    .iter()
                    .any(|k| k.name == name && k.running())
            {
                self.open = Some(name.clone());
                self.tab = Tab::Overview;
                self.awaited = None;
            }
        }
        let open = self.open.clone();
        let found = open
            .as_deref()
            .is_some_and(|name| clusters(ctx.store).iter().any(|k| k.name == name));
        match open {
            Some(name) if found => self.cluster_view(ui, ctx, &name),
            _ => {
                self.open = None;
                self.list(ui, ctx);
            }
        }
    }

    fn has_selection(&self, _store: &Store) -> bool {
        false
    }

    fn show_details(&mut self, _ui: &mut egui::Ui, _ctx: &mut PageCtx) {}

    fn clear_selection(&mut self) {
        self.open = None;
    }

    /// Opens the cluster with this name, or the Create dialog with `new`.
    fn select(&mut self, id: &str) {
        if id == "new" {
            self.form = Some(ClusterForm::new(&Store::new()));
        } else {
            self.open = Some(id.to_owned());
            self.tab = Tab::Overview;
        }
    }

    fn commands(&self, store: &Store) -> Vec<Command> {
        let mut list: Vec<Command> = clusters(store)
            .iter()
            .map(|k| Command::select(format!("Open {}", k.name), "Kubernetes cluster", k.name))
            .collect();
        list.push(Command::select(
            "New Kubernetes cluster…",
            "Kubernetes",
            "new",
        ));
        list
    }
}

impl KubePage {
    fn list(&mut self, ui: &mut egui::Ui, ctx: &mut PageCtx) {
        let p = ctx.palette;
        let store = ctx.store;
        let all = clusters(store);
        let shown: Vec<&Cluster> = all
            .iter()
            .filter(|k| ctx.matches_search(&[k.name]))
            .collect();
        // Being made, before their server shows up.
        let mut starting: Vec<&str> = store
            .pending
            .keys()
            .filter_map(|k| k.strip_prefix("kube:"))
            .filter(|name| !all.iter().any(|k| k.name == *name))
            .collect();
        starting.sort();
        let mut failed: Vec<(&String, &String)> = store
            .kube_errors
            .iter()
            .filter(|(name, _)| {
                !self.dismissed.contains(*name) && !all.iter().any(|k| k.name == name.as_str())
            })
            .collect();
        failed.sort();

        if all.is_empty() && starting.is_empty() && failed.is_empty() {
            if widgets::empty_state(
                ui,
                p,
                Icon::Wheel,
                "No Kubernetes cluster",
                "Create one when you need it: k3s in containers, ready in under a minute. \
                 Until then nothing runs and no memory is used.",
                Some("Create cluster…"),
            ) {
                self.form = Some(ClusterForm::new(store));
            }
            return;
        }
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                section(ui, p, "CLUSTERS");
                for name in &starting {
                    let step = store.kube_steps.get(*name).map_or_else(
                        || match store.pulls.iter().find(|(_, pr)| !pr.pushing) {
                            Some((image, pr)) if pr.total > 0 => format!(
                                "Downloading {image}… {:.0}%",
                                pr.done as f32 * 100.0 / pr.total as f32
                            ),
                            Some((image, _)) => format!("Downloading {image}…"),
                            None => "Starting…".to_owned(),
                        },
                        Clone::clone,
                    );
                    row_frame(ui, p, |ui| {
                        ui.spinner();
                        ui.label(RichText::new(*name).strong().color(p.text));
                        ui.label(RichText::new(step).size(12.5).color(p.muted));
                    });
                }
                for (name, message) in &failed {
                    let mut dismiss = false;
                    row_frame(ui, p, |ui| {
                        ui.label(RichText::new(name.as_str()).strong().color(p.text));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            dismiss = widgets::icon_button(ui, Icon::Close, "Dismiss").clicked();
                            ui.add(
                                egui::Label::new(
                                    RichText::new(message.as_str()).size(12.5).color(p.danger),
                                )
                                .truncate(),
                            )
                            .on_hover_text(message.as_str());
                        });
                    });
                    if dismiss {
                        self.dismissed.insert((*name).clone());
                    }
                }
                for k in &shown {
                    self.cluster_row(ui, ctx, k);
                }
                ui.add_space(6.0);
                ui.label(
                    RichText::new(
                        "Clusters run k3s, the lightweight Kubernetes, in containers. A \
                         stopped cluster uses no memory.",
                    )
                    .size(12.0)
                    .color(p.muted),
                );
            });
    }

    fn cluster_row(&mut self, ui: &mut egui::Ui, ctx: &mut PageCtx, k: &Cluster) {
        let p = ctx.palette;
        let busy = busy(ctx.store, k.name);
        row_frame(ui, p, |ui| {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(30.0, 30.0), egui::Sense::hover());
            ui.painter().rect_filled(
                rect,
                egui::CornerRadius::same(8),
                p.accent.gamma_multiply(0.12),
            );
            crate::app::components::icons::paint(
                ui.painter(),
                rect.shrink(6.0),
                Icon::Wheel,
                p.accent,
            );
            ui.vertical(|ui| {
                ui.set_max_width((ui.available_width() - 300.0).max(140.0));
                ui.label(RichText::new(k.name).strong().color(p.text));
                let mut about = format!("Kubernetes {} · {}", k.version(), k.nodes_text());
                if k.labels.ingress_port > 0 {
                    about.push_str(&format!(" · ingress :{}", k.labels.ingress_port));
                }
                ui.add(egui::Label::new(RichText::new(about).size(12.0).color(p.muted)).truncate());
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if busy {
                    let step = ctx
                        .store
                        .kube_steps
                        .get(k.name)
                        .map_or("Working…", String::as_str);
                    ui.label(RichText::new(step).size(12.5).color(p.muted));
                    ui.spinner();
                    return;
                }
                if widgets::icon_button(ui, Icon::Close, "Remove the cluster").clicked() {
                    ctx.request(confirm_remove(k));
                }
                if k.running() {
                    if widgets::icon_button(ui, Icon::Stop, "Stop the cluster").clicked() {
                        ctx.request(Intent::Kube(KubeOp::Stop(k.name.to_owned())));
                    }
                    if widgets::primary_button(ui, "Open").clicked() {
                        self.open = Some(k.name.to_owned());
                        self.tab = Tab::Overview;
                    }
                    widgets::pill(ui, "Running", p.success);
                } else {
                    if ui.button("Start").clicked() {
                        ctx.request(Intent::Kube(KubeOp::Start(k.name.to_owned())));
                    }
                    widgets::pill(ui, "Stopped", p.muted);
                }
            });
        });
    }

    fn cluster_view(&mut self, ui: &mut egui::Ui, ctx: &mut PageCtx, name: &str) {
        let p = ctx.palette;
        let all = clusters(ctx.store);
        let Some(k) = all.iter().find(|k| k.name == name) else {
            return;
        };
        let busy = busy(ctx.store, name);
        ui.horizontal(|ui| {
            if ui.button("← Clusters").clicked() {
                self.open = None;
            }
            ui.add_space(6.0);
            ui.label(
                RichText::new(name)
                    .font(egui::FontId::new(15.0, crate::theme::semibold()))
                    .color(p.text),
            );
            ui.label(
                RichText::new(format!("Kubernetes {} · {}", k.version(), k.nodes_text()))
                    .color(p.muted),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_enabled_ui(!busy, |ui| {
                    if k.running() {
                        if ui.button("Stop").clicked() {
                            ctx.request(Intent::Kube(KubeOp::Stop(name.to_owned())));
                        }
                    } else if widgets::primary_button(ui, "Start").clicked() {
                        ctx.request(Intent::Kube(KubeOp::Start(name.to_owned())));
                    }
                });
                if busy {
                    ui.spinner();
                }
            });
        });
        ui.add_space(4.0);
        access(ui, p, k.labels);
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            for (tab, label) in [
                (Tab::Overview, "Overview"),
                (Tab::Terminal, "Terminal"),
                (Tab::LoadImage, "Load image"),
            ] {
                if widgets::tab(ui, p, label, None, self.tab == tab).clicked() {
                    self.tab = tab;
                }
            }
        });
        ui.add_space(10.0);
        if !k.running() {
            ui.label(RichText::new("Start the cluster to see inside it.").color(p.muted));
            return;
        }
        match self.tab {
            Tab::Overview => self.overview(ui, ctx, name),
            Tab::Terminal => {
                if let Some(server) = k.server {
                    let server = server.clone();
                    ui.label(
                        RichText::new("A shell on the server node, with kubectl ready.")
                            .size(12.0)
                            .color(p.muted),
                    );
                    ui.add_space(6.0);
                    terminal::show(ui, ctx, &mut self.terminal, &server);
                }
            }
            Tab::LoadImage => self.load_image(ui, ctx, name),
        }
    }

    fn overview(&mut self, ui: &mut egui::Ui, ctx: &mut PageCtx, name: &str) {
        let p = ctx.palette;
        if self.asked.is_none_or(|t| t.elapsed() >= STATUS_EVERY) {
            self.asked = Some(Instant::now());
            ctx.request(Intent::Kube(KubeOp::Status(name.to_owned())));
        }
        ui.ctx().request_repaint_after(STATUS_EVERY);
        let status = match ctx.store.kube_status.get(name) {
            Some(Ok(status)) => status,
            Some(Err(e)) => {
                ui.label(RichText::new(format!("Couldn't read the cluster: {e}")).color(p.danger));
                return;
            }
            None => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(RichText::new("Reading the cluster…").color(p.muted));
                });
                return;
            }
        };
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| status_tables(ui, p, status));
    }

    fn load_image(&mut self, ui: &mut egui::Ui, ctx: &mut PageCtx, name: &str) {
        let p = ctx.palette;
        let mut tags: Vec<&str> = ctx
            .store
            .images
            .iter()
            .flat_map(|i| i.tags.iter().map(String::as_str))
            .filter(|t| !t.contains("<none>") && !t.contains("rancher/k3s:"))
            .collect();
        tags.sort_unstable();
        tags.dedup();
        ui.label(
            RichText::new(
                "Copy an image from Docker into every node, so pods can use it without a \
                 registry. Give the pod imagePullPolicy: IfNotPresent (or Never).",
            )
            .color(p.muted),
        );
        ui.add_space(10.0);
        let loading = ctx.store.pending.contains_key(&format!("kube-load:{name}"));
        ui.horizontal(|ui| {
            let label = ui.label(RichText::new("Image").color(p.muted));
            egui::ComboBox::from_id_salt("kube-load-image")
                .selected_text(if self.image.is_empty() {
                    "Pick an image"
                } else {
                    &self.image
                })
                .width(320.0)
                .show_ui(ui, |ui| {
                    for tag in &tags {
                        ui.selectable_value(&mut self.image, (*tag).to_owned(), *tag);
                    }
                })
                .response
                .labelled_by(label.id);
            let ready = !self.image.is_empty() && !loading;
            if ui
                .add_enabled_ui(ready, |ui| widgets::primary_button(ui, "Load"))
                .inner
                .clicked()
            {
                ctx.request(Intent::Kube(KubeOp::LoadImage {
                    cluster: name.to_owned(),
                    image: self.image.clone(),
                }));
            }
            if loading {
                ui.spinner();
                ui.label(RichText::new("Loading…").color(p.muted));
            }
        });
        if tags.is_empty() {
            ui.add_space(6.0);
            ui.label(
                RichText::new(
                    "There are no images here yet. Pull or build one on the Images page.",
                )
                .size(12.0)
                .color(p.muted),
            );
        }
    }
}

/// How to reach the cluster: kubectl, the API and the ingress.
fn access(ui: &mut egui::Ui, p: &Palette, labels: &KubeLabels) {
    let host = crate::docker::engine_host();
    ui.horizontal_wrapped(|ui| {
        if !labels.context.is_empty() {
            ui.label(RichText::new("kubectl").size(12.5).color(p.muted));
            let command = format!("kubectl config use-context {}", labels.context);
            ui.label(
                RichText::new(&labels.context)
                    .monospace()
                    .size(12.5)
                    .color(p.text),
            );
            if ui.small_button("Copy").on_hover_text(&command).clicked() {
                ui.ctx().copy_text(command);
            }
            ui.add_space(10.0);
        }
        if labels.ingress_port > 0 {
            let url = format!("http://{host}:{}", labels.ingress_port);
            ui.label(RichText::new("Ingress").size(12.5).color(p.muted));
            ui.add(egui::Hyperlink::from_label_and_url(
                RichText::new(&url).size(12.5),
                &url,
            ));
            ui.add_space(10.0);
        }
        if labels.api_port > 0 {
            ui.label(RichText::new("API").size(12.5).color(p.muted));
            ui.label(
                RichText::new(format!("https://{host}:{}", labels.api_port))
                    .monospace()
                    .size(12.5)
                    .color(p.text),
            );
        }
    });
}

fn status_tables(ui: &mut egui::Ui, p: &Palette, status: &ClusterStatus) {
    section(ui, p, "NODES");
    egui::Grid::new("kube-nodes")
        .num_columns(4)
        .spacing([28.0, 8.0])
        .striped(true)
        .show(ui, |ui| {
            for h in ["NAME", "ROLE", "STATE", "VERSION"] {
                ui.label(RichText::new(h).size(11.0).color(p.muted));
            }
            ui.end_row();
            for node in &status.nodes {
                ui.label(RichText::new(&node.name).color(p.text));
                let role = if node.roles.is_empty() {
                    "worker"
                } else {
                    "server"
                };
                ui.label(RichText::new(role).color(p.muted));
                if node.ready {
                    widgets::pill(ui, "Ready", p.success);
                } else {
                    widgets::pill(ui, "Not ready", p.warning);
                }
                ui.label(RichText::new(&node.version).color(p.muted));
                ui.end_row();
            }
        });
    ui.add_space(16.0);
    let healthy = status.pods.iter().filter(|p| p.healthy()).count();
    section(
        ui,
        p,
        &format!("PODS · {healthy} of {} running or done", status.pods.len()),
    );
    egui::Grid::new("kube-pods")
        .num_columns(6)
        .spacing([24.0, 8.0])
        .striped(true)
        .show(ui, |ui| {
            for h in ["NAMESPACE", "NAME", "READY", "STATUS", "RESTARTS", "NODE"] {
                ui.label(RichText::new(h).size(11.0).color(p.muted));
            }
            ui.end_row();
            for pod in &status.pods {
                ui.label(RichText::new(&pod.namespace).color(p.muted));
                ui.label(RichText::new(&pod.name).color(p.text));
                ui.label(RichText::new(&pod.ready).color(p.text));
                widgets::pill(ui, &pod.status, pod_color(p, pod));
                ui.label(RichText::new(pod.restarts.to_string()).color(p.muted));
                ui.label(RichText::new(&pod.node).color(p.muted));
                ui.end_row();
            }
        });
}
