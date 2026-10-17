//! Images: listing.

use bollard::models::ImageSummary;
use bollard::query_parameters::ListImagesOptions;

use super::{DockerClient, describe, short_id};

#[derive(Debug, Clone, PartialEq)]
pub struct ImageInfo {
    pub id: String,
    /// `repo:tag` names, without the `<none>:<none>` placeholder.
    pub tags: Vec<String>,
    pub size: i64,
    /// Unix timestamp (seconds).
    pub created: i64,
}

impl ImageInfo {
    pub fn short_id(&self) -> &str {
        short_id(&self.id)
    }

    /// The main name to show; untagged (dangling) images show as `<none>`.
    pub fn name(&self) -> &str {
        self.tags.first().map_or("<none>", String::as_str)
    }
}

impl From<ImageSummary> for ImageInfo {
    fn from(i: ImageSummary) -> Self {
        Self {
            id: i.id,
            tags: i
                .repo_tags
                .into_iter()
                .filter(|t| t != "<none>:<none>")
                .collect(),
            size: i.size,
            created: i.created,
        }
    }
}

impl DockerClient {
    /// Lists top-level images, sorted by name (untagged ones last).
    pub async fn list_images(&self) -> Result<Vec<ImageInfo>, String> {
        let mut list: Vec<ImageInfo> = self
            .docker
            .list_images(None::<ListImagesOptions>)
            .await
            .map_err(describe)?
            .into_iter()
            .map(ImageInfo::from)
            .collect();
        list.sort_by_cached_key(|i| (i.tags.is_empty(), i.name().to_lowercase()));
        Ok(list)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drops_placeholder_tags() {
        let info = ImageInfo::from(ImageSummary {
            id: "sha256:0123456789abcdef".into(),
            repo_tags: vec!["<none>:<none>".into()],
            ..Default::default()
        });
        assert!(info.tags.is_empty());
        assert_eq!(info.name(), "<none>");
        assert_eq!(info.short_id(), "0123456789ab");
    }
}
