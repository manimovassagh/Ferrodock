//! The web console: a page in the browser, served by the emulator itself,
//! that shows what each service holds and the requests it answered. It
//! reads the same data as the AWS APIs, so what the CLI creates shows up
//! there.
//!
//! - `assets.rs`: the page's files, built into the program.
//! - `inventory.rs`: what every service holds, as JSON for the page.
//!
//! The page itself is in `crates/cloud/console/`, a file per job:
//! `console.js` (what every page shares), `catalog.js` (how each service
//! looks), `home.js`, `service.js` and `requests.js` (the pages), and a
//! file per service with a browser of its own (`s3.js`).

mod assets;
mod inventory;

pub use assets::serve;
pub use inventory::inventory;

/// Where the console is.
pub const PATH: &str = "/_ferrocloud/console/";
