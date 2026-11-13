//! ferrocloud: Ferrodock's local cloud emulator.
//!
//! One HTTP server on one port. Each request is matched to a service the
//! way AWS does (its headers say which), handled by that service, and
//! recorded for the request log.
//!
//! - `server.rs`: accepts connections and turns each request into a call.
//! - `route.rs`: works out which service and operation a request is for.
//! - `protocol.rs`: what the AWS protocols share: errors, request IDs, XML.
//! - `service.rs`: what a service is, and the list of those there are.
//! - `admin.rs`: Ferrodock's own API under `/_ferrocloud/`.
//! - `console/`: the web console, served under `/_ferrocloud/console/`.
//! - `events.rs`: the record of requests, kept and streamed live.
//! - `capture.rs`: what the record keeps of each request, secrets masked.
//! - `store.rs`: where data lives: a folder with a SQLite database.
//! - `clock.rs`: timestamps in the formats the APIs use.
//! - `query.rs`: the query protocol's form fields and XML replies.
//! - `sts/`, `iam/`: the services, one folder each.

pub mod admin;
pub mod capture;
pub mod clock;
pub mod console;
pub mod events;
pub mod iam;
pub mod protocol;
pub mod query;
pub mod route;
pub mod server;
pub mod service;
pub mod store;
pub mod sts;

/// The account every resource belongs to.
pub const ACCOUNT: &str = "000000000000";
/// The region of a request that names none.
pub const DEFAULT_REGION: &str = "us-east-1";
/// The port AWS emulators conventionally listen on.
pub const DEFAULT_PORT: u16 = 4566;
