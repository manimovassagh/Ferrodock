//! The Create dialog of a Kubernetes cluster: name, version, workers,
//! ports, and whether to add it to kubectl.

use eframe::egui::{self, CornerRadius, Frame, Margin, RichText, Stroke};

use crate::app::components::widgets;
use crate::app::store::Store;
use crate::docker::{ClusterSpec, KUBE_VERSIONS, context_name, port_free};
use crate::theme::Palette;

/// Most workers a cluster can be made with.
const MAX_WORKERS: u8 = 3;

pub struct ClusterForm {
    name: String,
    tag: String,
    workers: u8,
    /// Kept as text so half-typed values survive.
    api_port: String,
    ingress_port: String,
    kubectl: bool,
}

pub enum Outcome {
    Create(Box<ClusterSpec>),
    Cancel,
}

/// The first free port from `start`, skipping `taken`.
fn free_port(store: &Store, start: u16, taken: Option<u16>) -> u16 {
    (start..start.saturating_add(100))
        .find(|&p| Some(p) != taken && port_free(&store.containers, p))
        .unwrap_or(start)
}

impl ClusterForm {
    /// Named `dev` (numbered past the names taken), on free ports.
    pub fn new(store: &Store) -> Self {
        let name = std::iter::once("dev".to_owned())
            .chain((2..).map(|n| format!("dev-{n}")))
            .find(|name| !name_taken(store, name))
            .unwrap_or_default();
        let ingress = free_port(store, 8080, None);
        Self {
            name,
            tag: KUBE_VERSIONS[0].1.to_owned(),
            workers: 0,
            api_port: free_port(store, 6550, Some(ingress)).to_string(),
            ingress_port: ingress.to_string(),
            kubectl: true,
        }
    }

    /// The cluster to create, or what's wrong with the form.
    pub fn checked(&self, store: &Store) -> Result<ClusterSpec, String> {
        let name = self.name.trim();
        if !valid_name(name) {
            return Err(
                "Name it with lowercase letters, digits and -, starting with a letter".into(),
            );
        }
        if name_taken(store, name) {
            return Err(format!(
                "There's already a cluster or container named {name}"
            ));
        }
        let port = |text: &str, what: &str| match text.trim().parse::<u16>() {
            Ok(p) if p > 0 => Ok(p),
            _ => Err(format!("The {what} port isn't a port number")),
        };
        let api_port = port(&self.api_port, "API")?;
        let ingress_port = port(&self.ingress_port, "ingress")?;
        if api_port == ingress_port {
            return Err("The API and the ingress need ports of their own".into());
        }
        for (p, what) in [(api_port, "API"), (ingress_port, "ingress")] {
            if !port_free(&store.containers, p) {
                return Err(format!("Port {p} (for the {what}) is in use"));
            }
        }
        Ok(ClusterSpec {
            name: name.to_owned(),
            tag: self.tag.clone(),
            workers: self.workers,
            api_port,
            ingress_port,
            kubectl: self.kubectl,
        })
    }
}

/// Short, lowercase, and fit for a hostname and a kubectl context.
fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 30
        && name.starts_with(|c: char| c.is_ascii_lowercase())
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Whether a cluster, one being made, or a container uses this name.
pub fn name_taken(store: &Store, name: &str) -> bool {
    let server = crate::docker::server_name(name);
    store.pending.contains_key(&format!("kube:{name}"))
        || store.containers.iter().any(|c| {
            c.name == name || c.name == server || c.kube.as_ref().is_some_and(|k| k.cluster == name)
        })
}

/// Draws the dialog. Returns what the user chose, once they choose.
pub fn show(
    ui: &mut egui::Ui,
    p: &Palette,
    store: &Store,
    form: &mut ClusterForm,
) -> Option<Outcome> {
    let mut outcome = None;
    let modal = egui::Modal::new(egui::Id::unique("ferrodock-kube"))
        .frame(
            Frame::new()
                .fill(p.sidebar)
                .stroke(Stroke::new(1.0, p.border))
                .corner_radius(CornerRadius::same(12))
                .inner_margin(Margin::same(20)),
        )
        .show(ui.ctx(), |ui| {
            ui.set_width(500.0);
            ui.label(
                RichText::new("New Kubernetes cluster")
                    .font(egui::FontId::new(18.0, crate::theme::semibold()))
                    .color(p.text),
            );
            ui.label(
                RichText::new("k3s, the lightweight Kubernetes, in containers on this engine.")
                    .size(12.5)
                    .color(p.muted),
            );
            ui.add_space(14.0);
            egui::Grid::new("kube-form")
                .num_columns(2)
                .spacing([16.0, 10.0])
                .show(ui, |ui| {
                    let label = ui.label(RichText::new("Name").color(p.muted));
                    ui.add(egui::TextEdit::singleline(&mut form.name).desired_width(f32::INFINITY))
                        .labelled_by(label.id);
                    ui.end_row();

                    ui.label(RichText::new("Kubernetes").color(p.muted));
                    let shown = KUBE_VERSIONS
                        .iter()
                        .find(|(_, tag)| *tag == form.tag)
                        .map_or(form.tag.as_str(), |(v, _)| v);
                    egui::ComboBox::from_id_salt("kube-version")
                        .selected_text(shown)
                        .show_ui(ui, |ui| {
                            for (version, tag) in KUBE_VERSIONS {
                                ui.selectable_value(&mut form.tag, (*tag).to_owned(), *version);
                            }
                        });
                    ui.end_row();

                    ui.label(RichText::new("Worker nodes").color(p.muted));
                    ui.horizontal(|ui| {
                        egui::ComboBox::from_id_salt("kube-workers")
                            .selected_text(form.workers.to_string())
                            .width(60.0)
                            .show_ui(ui, |ui| {
                                for n in 0..=MAX_WORKERS {
                                    ui.selectable_value(&mut form.workers, n, n.to_string());
                                }
                            });
                        ui.label(
                            RichText::new("Besides the server, which runs pods too")
                                .size(12.0)
                                .color(p.muted),
                        );
                    });
                    ui.end_row();

                    let label = ui.label(RichText::new("Ingress port").color(p.muted));
                    ui.horizontal(|ui| {
                        ui.add(egui::TextEdit::singleline(&mut form.ingress_port).desired_width(70.0))
                            .labelled_by(label.id);
                        ui.label(
                            RichText::new("→ port 80 of the cluster, for your apps")
                                .size(12.0)
                                .color(p.muted),
                        );
                    });
                    ui.end_row();

                    let label = ui.label(RichText::new("API port").color(p.muted));
                    ui.add(egui::TextEdit::singleline(&mut form.api_port).desired_width(70.0))
                        .labelled_by(label.id);
                    ui.end_row();

                    ui.label("");
                    ui.checkbox(
                        &mut form.kubectl,
                        format!(
                            "Use it with kubectl, as the context {}",
                            context_name(form.name.trim())
                        ),
                    );
                    ui.end_row();
                });
            ui.add_space(12.0);
            let checked = form.checked(store);
            match &checked {
                Err(problem) => {
                    ui.label(RichText::new(problem).size(12.0).color(p.danger));
                }
                Ok(spec) => {
                    let mut text = format!(
                        "Gets {} if it isn't here. Uses about 500 MB of memory per node while it runs.",
                        spec.image()
                    );
                    if spec.kubectl {
                        text.push_str(" Adds the context to your kubeconfig and switches kubectl to it.");
                    }
                    ui.add(egui::Label::new(RichText::new(text).size(12.0).color(p.muted)).wrap());
                }
            }
            ui.add_space(10.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let create = ui
                    .add_enabled_ui(checked.is_ok(), |ui| widgets::primary_button(ui, "Create"))
                    .inner;
                if create.clicked()
                    && let Ok(spec) = checked
                {
                    outcome = Some(Outcome::Create(Box::new(spec)));
                }
                if ui.button("Cancel").clicked() {
                    outcome = Some(Outcome::Cancel);
                }
            });
        });
    if outcome.is_none() && modal.should_close() {
        outcome = Some(Outcome::Cancel);
    }
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checks_names_and_ports() {
        assert!(valid_name("dev-2"));
        assert!(!valid_name("Dev"));
        assert!(!valid_name("2dev"));
        assert!(!valid_name("my_cluster"));

        let mut store = Store::new();
        let mut form = ClusterForm::new(&store);
        let spec = form.checked(&store).unwrap();
        assert_eq!(spec.name, "dev");
        assert_ne!(spec.api_port, spec.ingress_port);

        store.containers.push(crate::docker::ContainerInfo {
            name: "dev-server".into(),
            kube: Some(crate::docker::KubeLabels {
                cluster: "dev".into(),
                server: true,
                ..Default::default()
            }),
            ..Default::default()
        });
        assert!(form.checked(&store).is_err());
        assert_eq!(ClusterForm::new(&store).name, "dev-2");

        form.name = "other".into();
        form.ingress_port = form.api_port.clone();
        assert!(form.checked(&store).is_err());
        form.ingress_port = "80x".into();
        assert!(form.checked(&store).is_err());
    }
}
