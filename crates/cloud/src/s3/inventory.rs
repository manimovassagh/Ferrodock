//! What S3 holds, as the web console lists it: its buckets, each with how
//! many objects it has and their size. The console browses a bucket's
//! objects through S3's own API.

use serde_json::json;

use crate::clock;
use crate::objects::{Problem, records};
use crate::service::{Item, Listing};
use crate::store::Store;

/// A size for people: `512 B`, `1.4 MB`.
pub fn size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

fn failed(problem: Problem) -> String {
    format!("{problem:?}")
}

pub fn inventory(store: &Store) -> Result<Vec<Listing>, String> {
    let mut items = Vec::new();
    for bucket in records::buckets(store).map_err(failed)? {
        let (objects, bytes) = records::usage(store, &bucket.name).map_err(failed)?;
        let created = clock::iso8601_millis(bucket.created);
        items.push(Item {
            cells: vec![
                bucket.name.clone(),
                objects.to_string(),
                size(bytes),
                bucket.versioning.label().to_owned(),
                created.clone(),
            ],
            detail: json!({
                "name": bucket.name,
                "region": bucket.region,
                "created": created,
                "versioning": bucket.versioning.label(),
                "objects": objects,
                "size": bytes,
            }),
        });
    }
    Ok(vec![Listing {
        title: "Buckets",
        columns: &["Name", "Objects", "Size", "Versioning", "Created"],
        items,
    }])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_read_well() {
        assert_eq!(size(0), "0 B");
        assert_eq!(size(512), "512 B");
        assert_eq!(size(1536), "1.5 KB");
        assert_eq!(size(5 * 1024 * 1024), "5.0 MB");
    }

    #[test]
    fn lists_buckets_with_their_usage() {
        let store = Store::open(None).unwrap();
        records::init(&store).unwrap();
        records::create_bucket(&store, "photos", "us-east-1").unwrap();
        let listings = inventory(&store).unwrap();
        assert_eq!(listings[0].title, "Buckets");
        let cells = &listings[0].items[0].cells;
        assert_eq!(cells[..4], ["photos", "0", "0 B", "Off"]);
        assert_eq!(listings[0].columns.len(), cells.len());
    }
}
