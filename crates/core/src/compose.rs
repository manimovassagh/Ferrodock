//! Docker Compose projects, reconstructed from container labels.
//!
//! Compose has no API of its own in the Engine: it labels every container it
//! creates. Grouping containers by those labels gives us the projects.

use std::collections::BTreeMap;

use super::ContainerInfo;

#[derive(Debug, Clone, PartialEq)]
pub struct ComposeProject {
    pub name: String,
    pub working_dir: String,
    pub config_files: Vec<String>,
    /// The project's containers, sorted by service name.
    pub containers: Vec<ContainerInfo>,
}

impl ComposeProject {
    pub fn running(&self) -> usize {
        self.containers.iter().filter(|c| c.is_active()).count()
    }

    /// Distinct service names, in order.
    pub fn services(&self) -> Vec<&str> {
        let mut services: Vec<&str> = self
            .containers
            .iter()
            .filter_map(|c| c.compose.as_ref().map(|l| l.service.as_str()))
            .collect();
        services.dedup();
        services
    }
}

/// Groups the containers that belong to Compose projects, sorted by
/// project name. Containers outside any project are ignored.
pub fn group_projects(containers: &[ContainerInfo]) -> Vec<ComposeProject> {
    let mut projects: BTreeMap<&str, ComposeProject> = BTreeMap::new();
    for container in containers {
        let Some(labels) = &container.compose else {
            continue;
        };
        let project = projects
            .entry(labels.project.as_str())
            .or_insert_with(|| ComposeProject {
                name: labels.project.clone(),
                working_dir: labels.working_dir.clone(),
                config_files: labels
                    .config_files
                    .split(',')
                    .map(str::trim)
                    .filter(|f| !f.is_empty())
                    .map(str::to_owned)
                    .collect(),
                containers: Vec::new(),
            });
        project.containers.push(container.clone());
    }
    let mut projects: Vec<ComposeProject> = projects.into_values().collect();
    for project in &mut projects {
        project.containers.sort_by_cached_key(|c| {
            let service = c.compose.as_ref().map(|l| l.service.clone());
            (service, c.name.clone())
        });
    }
    projects
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::containers::ComposeLabels;

    fn container(name: &str, state: &str, compose: Option<(&str, &str)>) -> ContainerInfo {
        ContainerInfo {
            id: format!("{name}-id"),
            name: name.into(),
            image: String::new(),
            image_id: String::new(),
            command: String::new(),
            state: state.into(),
            status: String::new(),
            created: 0,
            ports: Vec::new(),
            ip_addresses: Vec::new(),
            networks: Vec::new(),
            mounts: Vec::new(),
            volumes: Vec::new(),
            compose: compose.map(|(project, service)| ComposeLabels {
                project: project.into(),
                service: service.into(),
                working_dir: "/srv/shop".into(),
                config_files: "/srv/shop/compose.yaml, /srv/shop/compose.override.yaml".into(),
            }),
            machine: None,
        }
    }

    #[test]
    fn groups_by_project_and_ignores_loose_containers() {
        let projects = group_projects(&[
            container("shop-web-1", "running", Some(("shop", "web"))),
            container("loose", "running", None),
            container("shop-db-1", "exited", Some(("shop", "db"))),
            container("blog-app-1", "running", Some(("blog", "app"))),
        ]);
        let names: Vec<&str> = projects.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, vec!["blog", "shop"]);

        let shop = &projects[1];
        assert_eq!(shop.services(), vec!["db", "web"]);
        assert_eq!(shop.running(), 1);
        assert_eq!(
            shop.config_files,
            vec!["/srv/shop/compose.yaml", "/srv/shop/compose.override.yaml"]
        );
    }
}
