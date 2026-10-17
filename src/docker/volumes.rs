//! Volumes: listing.

use bollard::models::Volume;
use bollard::query_parameters::ListVolumesOptions;

use super::{DockerClient, describe};

#[derive(Debug, Clone, PartialEq)]
pub struct VolumeInfo {
    pub name: String,
    pub driver: String,
    pub mountpoint: String,
    pub scope: String,
    /// Unix timestamp (seconds), when Docker reports one.
    pub created: Option<i64>,
}

impl From<Volume> for VolumeInfo {
    fn from(v: Volume) -> Self {
        Self {
            name: v.name,
            driver: v.driver,
            mountpoint: v.mountpoint,
            scope: v.scope.map(|s| s.to_string()).unwrap_or_default(),
            created: v.created_at.map(|t| t.unix_timestamp()),
        }
    }
}

impl DockerClient {
    /// Lists volumes, sorted by name.
    pub async fn list_volumes(&self) -> Result<Vec<VolumeInfo>, String> {
        let mut list: Vec<VolumeInfo> = self
            .docker
            .list_volumes(None::<ListVolumesOptions>)
            .await
            .map_err(describe)?
            .volumes
            .unwrap_or_default()
            .into_iter()
            .map(VolumeInfo::from)
            .collect();
        list.sort_by_cached_key(|v| v.name.to_lowercase());
        Ok(list)
    }
}
