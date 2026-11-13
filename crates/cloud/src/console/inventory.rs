//! What every service holds, in one answer, for the console's pages.

use serde_json::{Value, json};

use crate::server::App;

/// Each service with its listings. A service that can't be read says why
/// instead, and the others are still there.
pub fn inventory(app: &App) -> Value {
    let services = app
        .registry
        .services()
        .iter()
        .map(|service| {
            let (listings, error) = match service.inventory(&app.store) {
                Ok(listings) => (listings, None),
                Err(error) => (Vec::new(), Some(error)),
            };
            json!({
                "name": service.name(),
                "title": service.title(),
                "operations": service.operations().len(),
                "listings": listings,
                "error": error,
            })
        })
        .collect::<Vec<_>>();
    json!({ "services": services })
}
