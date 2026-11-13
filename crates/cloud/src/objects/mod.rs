//! Object storage: buckets of named objects, with versions and uploads in
//! parts. Nothing here knows about AWS: S3 is one way in, and other
//! clouds' object stores can be others.
//!
//! - `records.rs`: buckets, objects, versions and uploads, in SQLite.
//! - `files.rs`: objects' bytes, as files, written and read in pieces.
//! - `md5.rs`: the hash ETags are made of.

pub mod files;
pub mod md5;
pub mod records;

pub use files::{Writer, Written};
pub use records::{
    Bucket, Deleted, Listing, Meta, Object, Part, Problem, Upload, VersionListing, Versioning,
};
