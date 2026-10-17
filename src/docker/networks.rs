//! Networks: listing.

use bollard::models::Network;
use bollard::query_parameters::ListNetworksOptions;

use super::{DockerClient, describe, short_id};

#[derive(Debug, Clone, PartialEq)]
pub struct NetworkInfo {
    pub id: String,
    pub name: String,
    pub driver: String,
    pub scope: String,
    pub subnets: Vec<String>,
    pub gateways: Vec<String>,
    pub internal: bool,
    /// Unix timestamp (seconds), when Docker reports one.
    pub created: Option<i64>,
}

impl NetworkInfo {
    pub fn short_id(&self) -> &str {
        short_id(&self.id)
    }

    /// The networks every Docker install has; they can't be removed.
    pub fn is_builtin(&self) -> bool {
        matches!(self.name.as_str(), "bridge" | "host" | "none")
    }
}

impl From<Network> for NetworkInfo {
    fn from(n: Network) -> Self {
        let configs = n.ipam.and_then(|ipam| ipam.config).unwrap_or_default();
        Self {
            id: n.id.unwrap_or_default(),
            name: n.name.unwrap_or_default(),
            driver: n.driver.unwrap_or_default(),
            scope: n.scope.unwrap_or_default(),
            subnets: configs.iter().filter_map(|c| c.subnet.clone()).collect(),
            gateways: configs.iter().filter_map(|c| c.gateway.clone()).collect(),
            internal: n.internal.unwrap_or(false),
            created: n.created.map(|t| t.unix_timestamp()),
        }
    }
}

impl DockerClient {
    /// Lists networks, sorted by name.
    pub async fn list_networks(&self) -> Result<Vec<NetworkInfo>, String> {
        let mut list: Vec<NetworkInfo> = self
            .docker
            .list_networks(None::<ListNetworksOptions>)
            .await
            .map_err(describe)?
            .into_iter()
            .map(NetworkInfo::from)
            .collect();
        list.sort_by_cached_key(|n| n.name.to_lowercase());
        Ok(list)
    }
}
