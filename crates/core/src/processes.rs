//! What runs inside a container (`docker top`), and how an image was built
//! (`docker history`).

use bollard::query_parameters::TopOptions;

use super::{DockerClient, describe};

/// The process table of one container.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Processes {
    pub container_id: String,
    pub titles: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

/// One layer of an image, newest first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layer {
    /// The Dockerfile instruction that made it, tidied up.
    pub instruction: String,
    pub size: i64,
    /// Unix timestamp (seconds).
    pub created: i64,
}

/// `/bin/sh -c #(nop)  CMD ["nginx"]` → `CMD ["nginx"]`, and
/// `/bin/sh -c apt-get …` → `RUN apt-get …`.
fn tidy(created_by: &str) -> String {
    let s = created_by.trim();
    if let Some(rest) = s.strip_prefix("/bin/sh -c #(nop)") {
        return rest.trim().to_owned();
    }
    if let Some(rest) = s.strip_prefix("/bin/sh -c ") {
        return format!("RUN {}", rest.trim());
    }
    // BuildKit records instructions as written, with a `|N` arg count
    // prefix for RUN sometimes.
    s.trim_start_matches(|c: char| c == '|' || c.is_ascii_digit())
        .trim()
        .to_owned()
}

impl DockerClient {
    pub async fn processes(&self, id: &str) -> Result<Processes, String> {
        let top = self
            .docker
            .top_processes(id, None::<TopOptions>)
            .await
            .map_err(describe)?;
        Ok(Processes {
            container_id: id.to_owned(),
            titles: top.titles.unwrap_or_default(),
            rows: top.processes.unwrap_or_default(),
        })
    }

    pub async fn history(&self, image: &str) -> Result<Vec<Layer>, String> {
        let items = self.docker.image_history(image).await.map_err(describe)?;
        Ok(items
            .into_iter()
            .map(|item| Layer {
                instruction: tidy(&item.created_by),
                size: item.size,
                created: item.created,
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tidies_instructions() {
        assert_eq!(
            tidy("/bin/sh -c #(nop)  CMD [\"nginx\"]"),
            "CMD [\"nginx\"]"
        );
        assert_eq!(tidy("/bin/sh -c apk add curl"), "RUN apk add curl");
        assert_eq!(
            tidy("|1 VERSION=2 /bin/sh -c make"),
            "VERSION=2 /bin/sh -c make"
        );
        assert_eq!(tidy("COPY . /app # buildkit"), "COPY . /app # buildkit");
    }

    /// Talks to a real daemon: `cargo test -- --ignored`.
    #[tokio::test]
    #[ignore]
    async fn live_daemon_lists_processes_and_history() {
        let client = DockerClient::connect().unwrap();
        let containers = client.list_containers().await.unwrap();
        if let Some(c) = containers.iter().find(|c| c.is_running()) {
            let top = client.processes(&c.id).await.unwrap();
            assert!(!top.titles.is_empty());
            assert!(!top.rows.is_empty());
        }
        if let Some(image) = client.list_images().await.unwrap().first() {
            assert!(!client.history(&image.id).await.unwrap().is_empty());
        }
    }
}
