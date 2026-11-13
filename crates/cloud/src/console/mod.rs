//! The web console: a page in the browser, served by the emulator itself,
//! that shows what each service holds and the requests it answered. It
//! reads the same data as the AWS APIs, so what the CLI creates shows up
//! there.
//!
//! - `assets.rs`: the page's files, built into the program.
//! - `inventory.rs`: what every service holds, as JSON for the page.
//!
//! The page itself is in `crates/cloud/console/`.

mod assets;
mod inventory;

pub use assets::serve;
pub use inventory::inventory;

/// Where the console is.
pub const PATH: &str = "/_ferrocloud/console/";
