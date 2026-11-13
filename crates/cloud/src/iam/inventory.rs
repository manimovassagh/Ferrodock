//! What IAM holds, as the web console lists it: roles, users, policies
//! and instance profiles. Access keys aren't listed: they're secrets.

use serde_json::Value;

use super::records::{self, POLICY, PROFILE, ROLE, USER};
use crate::service::{Item, Listing};
use crate::store::Store;

/// A field of a record as text, empty when it's missing.
fn text(doc: &Value, field: &str) -> String {
    doc[field].as_str().unwrap_or_default().to_owned()
}

/// How many entries an array or object field has.
fn count(doc: &Value, field: &str) -> String {
    let n = match &doc[field] {
        Value::Array(entries) => entries.len(),
        Value::Object(entries) => entries.len(),
        _ => 0,
    };
    n.to_string()
}

/// One listing of every record of `kind`, a row each from `cells`.
fn listing(
    store: &Store,
    kind: &str,
    title: &'static str,
    columns: &'static [&'static str],
    cells: fn(&Value) -> Vec<String>,
) -> Result<Listing, String> {
    let items = records::list(store, kind)
        .map_err(|fault| fault.message)?
        .into_iter()
        .map(|detail| Item {
            cells: cells(&detail),
            detail,
        })
        .collect();
    Ok(Listing {
        title,
        columns,
        items,
    })
}

pub fn inventory(store: &Store) -> Result<Vec<Listing>, String> {
    Ok(vec![
        listing(
            store,
            ROLE,
            "Roles",
            &["Name", "ARN", "Attached policies", "Created"],
            |doc| {
                vec![
                    text(doc, "name"),
                    text(doc, "arn"),
                    count(doc, "attached"),
                    text(doc, "created"),
                ]
            },
        )?,
        listing(
            store,
            USER,
            "Users",
            &["Name", "ARN", "Attached policies", "Created"],
            |doc| {
                vec![
                    text(doc, "name"),
                    text(doc, "arn"),
                    count(doc, "attached"),
                    text(doc, "created"),
                ]
            },
        )?,
        listing(
            store,
            POLICY,
            "Policies",
            &["Name", "ARN", "Version", "Updated"],
            |doc| {
                vec![
                    text(doc, "name"),
                    text(doc, "arn"),
                    text(doc, "default_version"),
                    text(doc, "updated"),
                ]
            },
        )?,
        listing(
            store,
            PROFILE,
            "Instance profiles",
            &["Name", "ARN", "Roles", "Created"],
            |doc| {
                vec![
                    text(doc, "name"),
                    text(doc, "arn"),
                    count(doc, "roles"),
                    text(doc, "created"),
                ]
            },
        )?,
    ])
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn lists_what_iam_holds() {
        let store = Store::open(None).unwrap();
        records::init(&store).unwrap();
        let role = json!({
            "name": "shop-api",
            "arn": "arn:aws:iam::000000000000:role/shop-api",
            "created": "2026-11-13T10:00:00Z",
            "attached": ["arn:aws:iam::000000000000:policy/read-orders"],
        });
        records::put(&store, ROLE, "shop-api", &role).unwrap();

        let listings = inventory(&store).unwrap();
        let titles: Vec<_> = listings.iter().map(|l| l.title).collect();
        assert_eq!(titles, ["Roles", "Users", "Policies", "Instance profiles"]);
        let roles = &listings[0];
        assert_eq!(roles.columns.len(), roles.items[0].cells.len());
        assert_eq!(
            roles.items[0].cells,
            [
                "shop-api",
                "arn:aws:iam::000000000000:role/shop-api",
                "1",
                "2026-11-13T10:00:00Z"
            ]
        );
        assert_eq!(roles.items[0].detail, role);
        assert!(listings[1].items.is_empty());
    }
}
