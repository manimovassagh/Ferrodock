//! Cloud: Ferrodock's local cloud emulator, `ferrocloud`, run as a
//! container only when asked. It speaks the AWS APIs on one port, so the
//! AWS CLI, the SDKs and Terraform work against it unchanged.
//!
//! - `image.rs`: the emulator's image, downloaded once and checked.
//! - `container.rs`: starting, stopping and removing its container.
//! - `api.rs`: asking the running emulator what it has, and resetting it.
//! - `connect.rs`: the settings each tool needs to reach it.

mod api;
mod connect;
mod container;
mod image;

pub use api::{
    CloudBody, CloudDetail, CloudInfo, CloudRequest, CloudService, endpoint, info, request_detail,
    requests, reset,
};
pub use connect::{SNIPPETS, Snippet, env_lines};
pub use image::image_version;

/// Marks the emulator's container and volume; the value is the port of
/// this computer it's on.
pub const CLOUD_LABEL: &str = "dev.ferrodock.cloud";
/// `true` on a container that keeps its data in the volume.
pub const CLOUD_KEEP_LABEL: &str = "dev.ferrodock.cloud.keep";
/// The emulator's container.
pub const CLOUD_CONTAINER: &str = "ferrodock-cloud";
/// The volume its data is kept in.
pub const CLOUD_VOLUME: &str = "ferrodock-cloud-data";
/// The port AWS emulators conventionally listen on.
pub const CLOUD_PORT: u16 = 4566;
/// The port the emulator listens on inside its container.
const INNER_PORT: &str = "4566/tcp";
/// Where the emulator keeps its data inside its container.
const DATA_DIR: &str = "/var/lib/ferrocloud";
/// The account every resource belongs to.
pub const CLOUD_ACCOUNT: &str = "000000000000";
/// The region tools are pointed at.
pub const CLOUD_REGION: &str = "us-east-1";

/// How to run the emulator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CloudSpec {
    /// The port of this computer it's reached on.
    pub port: u16,
    /// Keep its data in a volume between starts, rather than start empty.
    pub keep_data: bool,
}

impl Default for CloudSpec {
    fn default() -> Self {
        Self {
            port: CLOUD_PORT,
            keep_data: true,
        }
    }
}

/// Something to do with the emulator, done by the app's worker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloudOp {
    Start(CloudSpec),
    Stop,
    /// Remove its container and the data it kept.
    Remove,
    /// Empty it: everything created in it goes, the emulator stays up.
    Reset(u16),
    /// Ask the emulator on this port what it has.
    Info(u16),
    /// Read the requests it answered after this one.
    Requests {
        port: u16,
        after: u64,
    },
    /// Read a request's headers and bodies.
    Detail {
        port: u16,
        id: u64,
    },
}

/// What the emulator's container records in its labels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct CloudLabels {
    pub port: u16,
    pub keep_data: bool,
}

impl CloudLabels {
    pub fn from_labels(labels: &std::collections::HashMap<String, String>) -> Option<Self> {
        Some(Self {
            port: labels.get(CLOUD_LABEL)?.parse().unwrap_or(CLOUD_PORT),
            keep_data: labels.get(CLOUD_KEEP_LABEL).is_some_and(|k| k == "true"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_its_labels() {
        let labels = |pairs: &[(&str, &str)]| {
            pairs
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                .collect::<std::collections::HashMap<_, _>>()
        };
        assert_eq!(CloudLabels::from_labels(&labels(&[("other", "1")])), None);
        assert_eq!(
            CloudLabels::from_labels(&labels(&[
                (CLOUD_LABEL, "4570"),
                (CLOUD_KEEP_LABEL, "true")
            ])),
            Some(CloudLabels {
                port: 4570,
                keep_data: true
            })
        );
        assert_eq!(
            CloudLabels::from_labels(&labels(&[(CLOUD_LABEL, "?")])),
            Some(CloudLabels {
                port: CLOUD_PORT,
                keep_data: false
            })
        );
    }
}
