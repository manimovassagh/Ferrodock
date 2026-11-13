//! Buckets, objects, their versions and uploads in parts, in SQLite. An
//! object's bytes are in a file (see `files.rs`); its record here says
//! which.

use std::collections::BTreeMap;

use rusqlite::{OptionalExtension, Row, params};
use serde::{Deserialize, Serialize};

use super::files::{self, Written};
use super::md5::{Md5, hex};
use crate::clock;
use crate::store::Store;

/// The version of an object in a bucket that doesn't keep versions.
pub const NO_VERSION: &str = "null";

/// What can go wrong, for each way in to word as its API does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Problem {
    NoBucket(String),
    BucketExists(String),
    BucketNotEmpty(String),
    /// No such object. `marker` is the version of the delete marker that
    /// hides it, when that's why.
    NoObject {
        key: String,
        marker: Option<String>,
    },
    NoVersion {
        key: String,
        version: String,
    },
    NoUpload(String),
    /// A part named on completing an upload isn't right.
    BadPart(String),
    Internal(String),
}

impl From<rusqlite::Error> for Problem {
    fn from(e: rusqlite::Error) -> Self {
        Problem::Internal(e.to_string())
    }
}

/// Whether a bucket keeps old versions of its objects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Versioning {
    /// Never turned on: one version of each object, called `null`.
    #[default]
    Off,
    Enabled,
    /// Turned off again: old versions stay, new writes replace `null`.
    Suspended,
}

impl Versioning {
    fn stored(self) -> &'static str {
        match self {
            Versioning::Off => "",
            Versioning::Enabled => "Enabled",
            Versioning::Suspended => "Suspended",
        }
    }

    fn read(text: &str) -> Self {
        match text {
            "Enabled" => Versioning::Enabled,
            "Suspended" => Versioning::Suspended,
            _ => Versioning::Off,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bucket {
    pub name: String,
    /// When it was made, in milliseconds since 1970.
    pub created: u64,
    pub region: String,
    pub versioning: Versioning,
}

/// What's kept with an object besides its bytes.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Meta {
    pub content_type: String,
    /// Headers given back as they were sent: `cache-control`,
    /// `content-disposition`, `content-encoding`…, by lowercase name.
    pub headers: BTreeMap<String, String>,
    /// The caller's own metadata, by name.
    pub user: BTreeMap<String, String>,
    pub tags: BTreeMap<String, String>,
}

/// One version of an object, or a delete marker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Object {
    pub bucket: String,
    pub key: String,
    /// `null` in a bucket that doesn't keep versions.
    pub version: String,
    /// The version a plain read gets.
    pub latest: bool,
    /// A delete marker: it has no bytes, and hides the versions under it.
    pub marker: bool,
    pub size: u64,
    /// Without quotes: the MD5 in hex, or `<md5>-<parts>` for an upload
    /// made in parts.
    pub etag: String,
    /// In milliseconds since 1970.
    pub modified: u64,
    /// The ID of the file with its bytes; empty for a delete marker.
    pub file: String,
    pub meta: Meta,
}

/// What a delete did.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Deleted {
    /// The version removed, or of the delete marker made.
    pub version: Option<String>,
    /// A delete marker was made, or was what got removed.
    pub marker: bool,
}

/// A page of a bucket's objects.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Listing {
    pub objects: Vec<Object>,
    /// Keys rolled up to the delimiter, like folders.
    pub prefixes: Vec<String>,
    pub truncated: bool,
    /// The last key or prefix here: list after it for the next page.
    pub next: Option<String>,
}

/// A page of a bucket's versions and delete markers.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct VersionListing {
    /// By key, each key's newest first.
    pub versions: Vec<Object>,
    pub prefixes: Vec<String>,
    pub truncated: bool,
    /// The last key and version here.
    pub next: Option<(String, String)>,
}

/// An upload in parts, not yet complete.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Upload {
    pub id: String,
    pub bucket: String,
    pub key: String,
    pub started: u64,
    pub meta: Meta,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Part {
    pub number: u32,
    pub size: u64,
    /// The MD5 of its bytes, in hex.
    pub etag: String,
    pub modified: u64,
    pub file: String,
}

pub fn init(store: &Store) -> Result<(), String> {
    store
        .db()
        .execute_batch(
            "CREATE TABLE IF NOT EXISTS object_buckets (
                name       TEXT PRIMARY KEY,
                created    INTEGER NOT NULL,
                region     TEXT NOT NULL,
                versioning TEXT NOT NULL DEFAULT '',
                config     TEXT NOT NULL DEFAULT '{}'
            );
            CREATE TABLE IF NOT EXISTS objects (
                bucket   TEXT NOT NULL,
                key      TEXT NOT NULL,
                version  TEXT NOT NULL,
                seq      INTEGER NOT NULL,
                latest   INTEGER NOT NULL,
                marker   INTEGER NOT NULL,
                size     INTEGER NOT NULL,
                etag     TEXT NOT NULL,
                modified INTEGER NOT NULL,
                file     TEXT NOT NULL,
                meta     TEXT NOT NULL,
                PRIMARY KEY (bucket, key, version)
            );
            CREATE INDEX IF NOT EXISTS objects_latest ON objects (bucket, latest, key);
            CREATE TABLE IF NOT EXISTS object_uploads (
                id      TEXT PRIMARY KEY,
                bucket  TEXT NOT NULL,
                key     TEXT NOT NULL,
                started INTEGER NOT NULL,
                meta    TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS object_parts (
                upload   TEXT NOT NULL,
                number   INTEGER NOT NULL,
                size     INTEGER NOT NULL,
                etag     TEXT NOT NULL,
                modified INTEGER NOT NULL,
                file     TEXT NOT NULL,
                PRIMARY KEY (upload, number)
            );",
        )
        .map_err(|e| e.to_string())
}

fn new_id() -> String {
    crate::sts::random("", 32)
}

// Buckets.

fn bucket_row(row: &Row) -> rusqlite::Result<Bucket> {
    Ok(Bucket {
        name: row.get(0)?,
        created: row.get(1)?,
        region: row.get(2)?,
        versioning: Versioning::read(&row.get::<_, String>(3)?),
    })
}

const BUCKET: &str = "SELECT name, created, region, versioning FROM object_buckets";

pub fn create_bucket(store: &Store, name: &str, region: &str) -> Result<Bucket, Problem> {
    let db = store.db();
    let made = db.execute(
        "INSERT OR IGNORE INTO object_buckets (name, created, region) VALUES (?1, ?2, ?3)",
        params![name, clock::now_millis(), region],
    )?;
    if made == 0 {
        return Err(Problem::BucketExists(name.to_owned()));
    }
    Ok(db.query_row(&format!("{BUCKET} WHERE name = ?1"), [name], bucket_row)?)
}

pub fn bucket(store: &Store, name: &str) -> Result<Bucket, Problem> {
    store
        .db()
        .query_row(&format!("{BUCKET} WHERE name = ?1"), [name], bucket_row)
        .optional()?
        .ok_or_else(|| Problem::NoBucket(name.to_owned()))
}

pub fn buckets(store: &Store) -> Result<Vec<Bucket>, Problem> {
    let db = store.db();
    let mut query = db.prepare(&format!("{BUCKET} ORDER BY name"))?;
    let rows = query.query_map([], bucket_row)?.collect::<Result<_, _>>()?;
    Ok(rows)
}

/// Removes an empty bucket, and any uploads to it left unfinished.
pub fn delete_bucket(store: &Store, name: &str) -> Result<(), Problem> {
    bucket(store, name)?;
    let used: bool = store.db().query_row(
        "SELECT EXISTS (SELECT 1 FROM objects WHERE bucket = ?1)",
        [name],
        |row| row.get(0),
    )?;
    if used {
        return Err(Problem::BucketNotEmpty(name.to_owned()));
    }
    for upload in uploads(store, name)? {
        remove_upload(store, &upload.id)?;
    }
    store
        .db()
        .execute("DELETE FROM object_buckets WHERE name = ?1", [name])?;
    Ok(())
}

pub fn set_versioning(store: &Store, name: &str, versioning: Versioning) -> Result<(), Problem> {
    bucket(store, name)?;
    store.db().execute(
        "UPDATE object_buckets SET versioning = ?2 WHERE name = ?1",
        params![name, versioning.stored()],
    )?;
    Ok(())
}

fn configs(store: &Store, name: &str) -> Result<BTreeMap<String, String>, Problem> {
    let text: Option<String> = store
        .db()
        .query_row(
            "SELECT config FROM object_buckets WHERE name = ?1",
            [name],
            |row| row.get(0),
        )
        .optional()?;
    let text = text.ok_or_else(|| Problem::NoBucket(name.to_owned()))?;
    Ok(serde_json::from_str(&text).unwrap_or_default())
}

/// A document kept with a bucket under a name (its CORS rules, say), as
/// it was given.
pub fn config(store: &Store, bucket: &str, name: &str) -> Result<Option<String>, Problem> {
    Ok(configs(store, bucket)?.remove(name))
}

/// Keeps a document with a bucket, or with `None` forgets it.
pub fn set_config(
    store: &Store,
    bucket: &str,
    name: &str,
    document: Option<&str>,
) -> Result<(), Problem> {
    let mut all = configs(store, bucket)?;
    match document {
        Some(document) => all.insert(name.to_owned(), document.to_owned()),
        None => all.remove(name),
    };
    let text = serde_json::to_string(&all).map_err(|e| Problem::Internal(e.to_string()))?;
    store.db().execute(
        "UPDATE object_buckets SET config = ?2 WHERE name = ?1",
        params![bucket, text],
    )?;
    Ok(())
}

/// How many objects a bucket has (their current versions) and their size.
pub fn usage(store: &Store, bucket: &str) -> Result<(u64, u64), Problem> {
    Ok(store.db().query_row(
        "SELECT COUNT(*), COALESCE(SUM(size), 0) FROM objects WHERE bucket = ?1 AND latest = 1 AND marker = 0",
        [bucket],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?)
}

// Objects.

const OBJECT: &str =
    "SELECT bucket, key, version, latest, marker, size, etag, modified, file, meta FROM objects";

fn object_row(row: &Row) -> rusqlite::Result<Object> {
    Ok(Object {
        bucket: row.get(0)?,
        key: row.get(1)?,
        version: row.get(2)?,
        latest: row.get(3)?,
        marker: row.get(4)?,
        size: row.get(5)?,
        etag: row.get(6)?,
        modified: row.get(7)?,
        file: row.get(8)?,
        meta: serde_json::from_str(&row.get::<_, String>(9)?).unwrap_or_default(),
    })
}

fn find(
    db: &rusqlite::Connection,
    bucket: &str,
    key: &str,
    version: Option<&str>,
) -> rusqlite::Result<Option<Object>> {
    match version {
        Some(version) => db
            .query_row(
                &format!("{OBJECT} WHERE bucket = ?1 AND key = ?2 AND version = ?3"),
                [bucket, key, version],
                object_row,
            )
            .optional(),
        None => db
            .query_row(
                &format!("{OBJECT} WHERE bucket = ?1 AND key = ?2 AND latest = 1"),
                [bucket, key],
                object_row,
            )
            .optional(),
    }
}

/// Adds a row as the key's newest version.
fn insert(db: &rusqlite::Connection, object: &Object) -> Result<(), Problem> {
    db.execute(
        "UPDATE objects SET latest = 0 WHERE bucket = ?1 AND key = ?2",
        [&object.bucket, &object.key],
    )?;
    let meta = serde_json::to_string(&object.meta).map_err(|e| Problem::Internal(e.to_string()))?;
    db.execute(
        "INSERT INTO objects (bucket, key, version, seq, latest, marker, size, etag, modified, file, meta)
         VALUES (?1, ?2, ?3, (SELECT COALESCE(MAX(seq), 0) + 1 FROM objects), 1, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            object.bucket,
            object.key,
            object.version,
            object.marker,
            object.size,
            object.etag,
            object.modified,
            object.file,
            meta
        ],
    )?;
    Ok(())
}

/// Removes a row and gives back its file's ID, to delete once the
/// database no longer names it.
fn remove_row(db: &rusqlite::Connection, object: &Object) -> Result<String, Problem> {
    db.execute(
        "DELETE FROM objects WHERE bucket = ?1 AND key = ?2 AND version = ?3",
        [&object.bucket, &object.key, &object.version],
    )?;
    if object.latest {
        db.execute(
            "UPDATE objects SET latest = 1 WHERE bucket = ?1 AND key = ?2 AND seq =
                (SELECT MAX(seq) FROM objects WHERE bucket = ?1 AND key = ?2)",
            [&object.bucket, &object.key],
        )?;
    }
    Ok(object.file.clone())
}

/// Stores a written file as the newest version of `key`. In a bucket
/// that keeps versions the old ones stay; otherwise it replaces what was
/// there.
pub fn put(
    store: &Store,
    bucket_name: &str,
    key: &str,
    written: &Written,
    etag: &str,
    meta: &Meta,
) -> Result<Object, Problem> {
    let bucket = bucket(store, bucket_name)?;
    let version = match bucket.versioning {
        Versioning::Enabled => new_id(),
        Versioning::Off | Versioning::Suspended => NO_VERSION.to_owned(),
    };
    let object = Object {
        bucket: bucket_name.to_owned(),
        key: key.to_owned(),
        version,
        latest: true,
        marker: false,
        size: written.size,
        etag: etag.to_owned(),
        modified: clock::now_millis(),
        file: written.file.clone(),
        meta: meta.clone(),
    };
    let replaced = {
        let db = store.db();
        let old = match bucket.versioning {
            Versioning::Enabled => None,
            _ => find(&db, bucket_name, key, Some(NO_VERSION))?,
        };
        let replaced = old.map(|old| remove_row(&db, &old)).transpose()?;
        insert(&db, &object)?;
        replaced
    };
    if let Some(file) = replaced {
        files::remove(store, &file);
    }
    Ok(object)
}

/// An object: the version named, or the current one.
pub fn get(
    store: &Store,
    bucket_name: &str,
    key: &str,
    version: Option<&str>,
) -> Result<Object, Problem> {
    bucket(store, bucket_name)?;
    let found = find(&store.db(), bucket_name, key, version)?;
    match (found, version) {
        (Some(object), Some(_)) => Ok(object),
        (Some(object), None) if object.marker => Err(Problem::NoObject {
            key: key.to_owned(),
            marker: Some(object.version),
        }),
        (Some(object), None) => Ok(object),
        (None, Some(version)) => Err(Problem::NoVersion {
            key: key.to_owned(),
            version: version.to_owned(),
        }),
        (None, None) => Err(Problem::NoObject {
            key: key.to_owned(),
            marker: None,
        }),
    }
}

/// Deletes an object. With a version, that version goes for good.
/// Without one, a bucket that keeps versions gets a delete marker and
/// loses nothing; any other bucket loses the object.
pub fn delete(
    store: &Store,
    bucket_name: &str,
    key: &str,
    version: Option<&str>,
) -> Result<Deleted, Problem> {
    let bucket = bucket(store, bucket_name)?;
    let mut gone = Vec::new();
    let deleted = {
        let db = store.db();
        match (version, bucket.versioning) {
            (Some(version), _) => {
                let found = find(&db, bucket_name, key, Some(version))?;
                let marker = found.as_ref().is_some_and(|o| o.marker);
                if let Some(object) = found {
                    gone.push(remove_row(&db, &object)?);
                }
                Deleted {
                    version: Some(version.to_owned()),
                    marker,
                }
            }
            (None, Versioning::Off) => {
                if let Some(object) = find(&db, bucket_name, key, Some(NO_VERSION))? {
                    gone.push(remove_row(&db, &object)?);
                }
                Deleted::default()
            }
            (None, versioning) => {
                let version = if versioning == Versioning::Enabled {
                    new_id()
                } else {
                    // Suspended: the marker takes the place of `null`.
                    if let Some(object) = find(&db, bucket_name, key, Some(NO_VERSION))? {
                        gone.push(remove_row(&db, &object)?);
                    }
                    NO_VERSION.to_owned()
                };
                insert(
                    &db,
                    &Object {
                        bucket: bucket_name.to_owned(),
                        key: key.to_owned(),
                        version: version.clone(),
                        latest: true,
                        marker: true,
                        size: 0,
                        etag: String::new(),
                        modified: clock::now_millis(),
                        file: String::new(),
                        meta: Meta::default(),
                    },
                )?;
                Deleted {
                    version: Some(version),
                    marker: true,
                }
            }
        }
    };
    for file in gone {
        files::remove(store, &file);
    }
    Ok(deleted)
}

/// Replaces what's kept with a version of an object (its tags, say).
pub fn set_meta(store: &Store, object: &Object, meta: &Meta) -> Result<(), Problem> {
    let text = serde_json::to_string(meta).map_err(|e| Problem::Internal(e.to_string()))?;
    store.db().execute(
        "UPDATE objects SET meta = ?4 WHERE bucket = ?1 AND key = ?2 AND version = ?3",
        params![object.bucket, object.key, object.version, text],
    )?;
    Ok(())
}

/// The prefix a key rolls up to, if `delimiter` comes after `prefix` in
/// it: `photos/2026/` for `photos/2026/a.jpg` under `photos/` with `/`.
fn rolled_up<'a>(key: &'a str, prefix: &str, delimiter: &str) -> Option<&'a str> {
    if delimiter.is_empty() {
        return None;
    }
    let rest = key.get(prefix.len()..)?;
    let at = rest.find(delimiter)?;
    Some(&key[..prefix.len() + at + delimiter.len()])
}

/// A bucket's current objects under `prefix`, by key, after `after`, at
/// most `max` of them (objects and rolled-up prefixes together).
pub fn list(
    store: &Store,
    bucket_name: &str,
    prefix: &str,
    delimiter: &str,
    after: &str,
    max: usize,
) -> Result<Listing, Problem> {
    bucket(store, bucket_name)?;
    let db = store.db();
    let mut query = db.prepare(&format!(
        "{OBJECT} WHERE bucket = ?1 AND latest = 1 AND marker = 0 AND key > ?2 AND key >= ?3 ORDER BY key"
    ))?;
    let mut rows = query.query(params![bucket_name, after, prefix])?;
    let mut listing = Listing::default();
    while let Some(row) = rows.next()? {
        let object = object_row(row)?;
        if !object.key.starts_with(prefix) {
            // In key order, so everything under the prefix has been seen.
            break;
        }
        let rolled = rolled_up(&object.key, prefix, delimiter);
        // A prefix already given: on the page before, or just now.
        if rolled
            .is_some_and(|p| p == after || listing.prefixes.last().is_some_and(|last| last == p))
        {
            continue;
        }
        if listing.objects.len() + listing.prefixes.len() == max {
            listing.truncated = true;
            break;
        }
        match rolled {
            Some(prefix) => {
                listing.next = Some(prefix.to_owned());
                listing.prefixes.push(prefix.to_owned());
            }
            None => {
                listing.next = Some(object.key.clone());
                listing.objects.push(object);
            }
        }
    }
    if !listing.truncated {
        listing.next = None;
    }
    Ok(listing)
}

/// A bucket's versions and delete markers under `prefix`: by key, each
/// key's newest first, after the key (and version of it) given.
pub fn list_versions(
    store: &Store,
    bucket_name: &str,
    prefix: &str,
    delimiter: &str,
    after: (&str, Option<&str>),
    max: usize,
) -> Result<VersionListing, Problem> {
    bucket(store, bucket_name)?;
    let (after_key, after_version) = after;
    let db = store.db();
    let mut query = db.prepare(&format!(
        "{OBJECT} WHERE bucket = ?1 AND key >= ?2 AND key >= ?3 ORDER BY key, seq DESC"
    ))?;
    let mut rows = query.query(params![bucket_name, after_key, prefix])?;
    let mut listing = VersionListing::default();
    // Within the key to start after, versions are skipped up to and
    // including the one named; with none named, all of them.
    let mut skipping = !after_key.is_empty();
    while let Some(row) = rows.next()? {
        let object = object_row(row)?;
        if !object.key.starts_with(prefix) {
            break;
        }
        if skipping && object.key == after_key {
            if after_version.is_some_and(|v| v == object.version) {
                skipping = false;
            }
            continue;
        }
        skipping = false;
        let rolled = rolled_up(&object.key, prefix, delimiter);
        if rolled.is_some_and(|p| {
            p == after_key || listing.prefixes.last().is_some_and(|last| last == p)
        }) {
            continue;
        }
        if listing.versions.len() + listing.prefixes.len() == max {
            listing.truncated = true;
            break;
        }
        match rolled {
            Some(prefix) => {
                listing.next = Some((prefix.to_owned(), String::new()));
                listing.prefixes.push(prefix.to_owned());
            }
            None => {
                listing.next = Some((object.key.clone(), object.version.clone()));
                listing.versions.push(object);
            }
        }
    }
    if !listing.truncated {
        listing.next = None;
    }
    Ok(listing)
}

// Uploads in parts.

fn upload_row(row: &Row) -> rusqlite::Result<Upload> {
    Ok(Upload {
        id: row.get(0)?,
        bucket: row.get(1)?,
        key: row.get(2)?,
        started: row.get(3)?,
        meta: serde_json::from_str(&row.get::<_, String>(4)?).unwrap_or_default(),
    })
}

const UPLOAD: &str = "SELECT id, bucket, key, started, meta FROM object_uploads";

pub fn create_upload(
    store: &Store,
    bucket_name: &str,
    key: &str,
    meta: &Meta,
) -> Result<Upload, Problem> {
    bucket(store, bucket_name)?;
    let upload = Upload {
        id: new_id(),
        bucket: bucket_name.to_owned(),
        key: key.to_owned(),
        started: clock::now_millis(),
        meta: meta.clone(),
    };
    let meta = serde_json::to_string(meta).map_err(|e| Problem::Internal(e.to_string()))?;
    store.db().execute(
        "INSERT INTO object_uploads (id, bucket, key, started, meta) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![upload.id, upload.bucket, upload.key, upload.started, meta],
    )?;
    Ok(upload)
}

pub fn upload(store: &Store, id: &str) -> Result<Upload, Problem> {
    store
        .db()
        .query_row(&format!("{UPLOAD} WHERE id = ?1"), [id], upload_row)
        .optional()?
        .ok_or_else(|| Problem::NoUpload(id.to_owned()))
}

/// A bucket's unfinished uploads, by key.
pub fn uploads(store: &Store, bucket_name: &str) -> Result<Vec<Upload>, Problem> {
    bucket(store, bucket_name)?;
    let db = store.db();
    let mut query = db.prepare(&format!("{UPLOAD} WHERE bucket = ?1 ORDER BY key, started"))?;
    let rows = query
        .query_map([bucket_name], upload_row)?
        .collect::<Result<_, _>>()?;
    Ok(rows)
}

/// Keeps a written file as a part of an upload, in place of any part
/// with that number already there.
pub fn put_part(
    store: &Store,
    upload_id: &str,
    number: u32,
    written: &Written,
) -> Result<(), Problem> {
    upload(store, upload_id)?;
    let old: Option<String> = {
        let db = store.db();
        let old = db
            .query_row(
                "SELECT file FROM object_parts WHERE upload = ?1 AND number = ?2",
                params![upload_id, number],
                |row| row.get(0),
            )
            .optional()?;
        db.execute(
            "INSERT OR REPLACE INTO object_parts (upload, number, size, etag, modified, file)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                upload_id,
                number,
                written.size,
                written.md5,
                clock::now_millis(),
                written.file
            ],
        )?;
        old
    };
    if let Some(file) = old {
        files::remove(store, &file);
    }
    Ok(())
}

/// An upload's parts so far, by number.
pub fn parts(store: &Store, upload_id: &str) -> Result<Vec<Part>, Problem> {
    upload(store, upload_id)?;
    let db = store.db();
    let mut query = db.prepare(
        "SELECT number, size, etag, modified, file FROM object_parts WHERE upload = ?1 ORDER BY number",
    )?;
    let rows = query
        .query_map([upload_id], |row| {
            Ok(Part {
                number: row.get(0)?,
                size: row.get(1)?,
                etag: row.get(2)?,
                modified: row.get(3)?,
                file: row.get(4)?,
            })
        })?
        .collect::<Result<_, _>>()?;
    Ok(rows)
}

/// Forgets an upload and removes its parts.
pub fn remove_upload(store: &Store, id: &str) -> Result<(), Problem> {
    let parts = parts(store, id)?;
    {
        let db = store.db();
        db.execute("DELETE FROM object_parts WHERE upload = ?1", [id])?;
        db.execute("DELETE FROM object_uploads WHERE id = ?1", [id])?;
    }
    for part in parts {
        files::remove(store, &part.file);
    }
    Ok(())
}

/// The ETag of an object made of parts: the MD5 of their MD5s, then how
/// many there were.
fn parts_etag(parts: &[&Part]) -> String {
    let mut hash = Md5::default();
    for part in parts {
        let bytes: Vec<u8> = (0..part.etag.len() / 2)
            .filter_map(|i| u8::from_str_radix(part.etag.get(i * 2..i * 2 + 2)?, 16).ok())
            .collect();
        hash.update(&bytes);
    }
    format!("{}-{}", hex(&hash.finish()), parts.len())
}

/// Makes the object from the parts chosen (number and ETag, in order),
/// and ends the upload.
pub async fn complete_upload(
    store: &Store,
    id: &str,
    chosen: &[(u32, String)],
) -> Result<Object, Problem> {
    let upload = upload(store, id)?;
    let have = parts(store, id)?;
    if chosen.is_empty() {
        return Err(Problem::BadPart("no parts were listed".into()));
    }
    let mut picked = Vec::with_capacity(chosen.len());
    let mut last = 0;
    for (number, etag) in chosen {
        if *number <= last {
            return Err(Problem::BadPart(
                "the parts aren't in ascending order".into(),
            ));
        }
        last = *number;
        let part = have
            .iter()
            .find(|part| part.number == *number)
            .filter(|part| etag.is_empty() || part.etag == etag.trim_matches('"'))
            .ok_or_else(|| {
                Problem::BadPart(format!(
                    "part {number} wasn't uploaded, or its ETag doesn't match"
                ))
            })?;
        picked.push(part);
    }
    let ids: Vec<String> = picked.iter().map(|part| part.file.clone()).collect();
    let written = files::join(store, &ids).await.map_err(Problem::Internal)?;
    let object = put(
        store,
        &upload.bucket,
        &upload.key,
        &written,
        &parts_etag(&picked),
        &upload.meta,
    );
    if object.is_err() {
        files::remove(store, &written.file);
    }
    let object = object?;
    remove_upload(store, id)?;
    Ok(object)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::objects::files::Writer;

    fn store() -> Store {
        let store = Store::open(None).unwrap();
        init(&store).unwrap();
        store
    }

    async fn write(store: &Store, text: &str) -> Written {
        let mut writer = Writer::create(store).await.unwrap();
        writer.write(text.as_bytes()).await.unwrap();
        writer.finish().await.unwrap()
    }

    async fn add(store: &Store, bucket: &str, key: &str, text: &str) -> Object {
        let written = write(store, text).await;
        put(store, bucket, key, &written, &written.md5, &Meta::default()).unwrap()
    }

    fn keys(listing: &Listing) -> Vec<&str> {
        listing.objects.iter().map(|o| o.key.as_str()).collect()
    }

    #[tokio::test]
    async fn keeps_buckets_and_objects() {
        let store = store();
        create_bucket(&store, "photos", "eu-west-1").unwrap();
        assert_eq!(
            create_bucket(&store, "photos", "x").unwrap_err(),
            Problem::BucketExists("photos".into())
        );
        assert_eq!(bucket(&store, "photos").unwrap().region, "eu-west-1");
        assert_eq!(
            bucket(&store, "none").unwrap_err(),
            Problem::NoBucket("none".into())
        );

        let first = add(&store, "photos", "a.txt", "one").await;
        assert_eq!((first.version.as_str(), first.size), (NO_VERSION, 3));
        // Written again, the old bytes go.
        let second = add(&store, "photos", "a.txt", "three").await;
        assert!(!files::path(&store, &first.file).exists());
        assert_eq!(get(&store, "photos", "a.txt", None).unwrap(), second);
        assert_eq!(usage(&store, "photos").unwrap(), (1, 5));

        assert_eq!(
            delete_bucket(&store, "photos").unwrap_err(),
            Problem::BucketNotEmpty("photos".into())
        );
        assert_eq!(
            delete(&store, "photos", "a.txt", None).unwrap(),
            Deleted::default()
        );
        assert!(!files::path(&store, &second.file).exists());
        assert!(matches!(
            get(&store, "photos", "a.txt", None),
            Err(Problem::NoObject { marker: None, .. })
        ));
        // Deleting what isn't there is fine.
        delete(&store, "photos", "a.txt", None).unwrap();
        delete_bucket(&store, "photos").unwrap();
        assert!(buckets(&store).unwrap().is_empty());
    }

    #[tokio::test]
    async fn lists_with_prefixes_delimiters_and_pages() {
        let store = store();
        create_bucket(&store, "b", "r").unwrap();
        for key in [
            "a.txt",
            "docs/one.md",
            "docs/two.md",
            "docs/img/x.png",
            "photos/1.jpg",
            "z.txt",
        ] {
            add(&store, "b", key, "x").await;
        }
        let all = list(&store, "b", "", "", "", 1000).unwrap();
        assert_eq!(
            keys(&all),
            [
                "a.txt",
                "docs/img/x.png",
                "docs/one.md",
                "docs/two.md",
                "photos/1.jpg",
                "z.txt"
            ]
        );
        assert!(!all.truncated && all.next.is_none());

        let top = list(&store, "b", "", "/", "", 1000).unwrap();
        assert_eq!(keys(&top), ["a.txt", "z.txt"]);
        assert_eq!(top.prefixes, ["docs/", "photos/"]);
        let docs = list(&store, "b", "docs/", "/", "", 1000).unwrap();
        assert_eq!(keys(&docs), ["docs/one.md", "docs/two.md"]);
        assert_eq!(docs.prefixes, ["docs/img/"]);

        // Pages of two, prefixes counted, none given twice.
        let one = list(&store, "b", "", "/", "", 2).unwrap();
        assert_eq!(
            (keys(&one), one.prefixes.clone()),
            (vec!["a.txt"], vec!["docs/".to_owned()])
        );
        assert!(one.truncated);
        let two = list(&store, "b", "", "/", one.next.as_deref().unwrap(), 2).unwrap();
        assert_eq!(
            (keys(&two), two.prefixes.clone()),
            (vec!["z.txt"], vec!["photos/".to_owned()])
        );
        assert!(!two.truncated);
        assert!(
            list(&store, "b", "nothing/", "/", "", 10)
                .unwrap()
                .objects
                .is_empty()
        );
    }

    #[tokio::test]
    async fn keeps_versions_when_asked() {
        let store = store();
        create_bucket(&store, "b", "r").unwrap();
        let plain = add(&store, "b", "k", "v0").await;
        set_versioning(&store, "b", Versioning::Enabled).unwrap();
        let v1 = add(&store, "b", "k", "v1").await;
        let v2 = add(&store, "b", "k", "v2").await;
        assert_ne!(v1.version, v2.version);
        assert_eq!(get(&store, "b", "k", None).unwrap().version, v2.version);
        assert_eq!(
            get(&store, "b", "k", Some(NO_VERSION)).unwrap().file,
            plain.file
        );

        // A plain delete hides it behind a marker and loses nothing.
        let hidden = delete(&store, "b", "k", None).unwrap();
        assert!(hidden.marker);
        let marker = hidden.version.unwrap();
        assert_eq!(
            get(&store, "b", "k", None).unwrap_err(),
            Problem::NoObject {
                key: "k".into(),
                marker: Some(marker.clone())
            }
        );
        assert!(
            list(&store, "b", "", "", "", 10)
                .unwrap()
                .objects
                .is_empty()
        );
        let versions = list_versions(&store, "b", "", "", ("", None), 10)
            .unwrap()
            .versions;
        assert_eq!(
            versions
                .iter()
                .map(|v| (v.version.as_str(), v.marker, v.latest))
                .collect::<Vec<_>>(),
            [
                (marker.as_str(), true, true),
                (v2.version.as_str(), false, false),
                (v1.version.as_str(), false, false),
                (NO_VERSION, false, false),
            ]
        );
        // Paged, from after a version.
        let page = list_versions(&store, "b", "", "", ("", None), 2).unwrap();
        assert!(page.truncated);
        let (key, version) = page.next.unwrap();
        let rest = list_versions(&store, "b", "", "", (&key, Some(&version)), 10).unwrap();
        assert_eq!(rest.versions.len(), 2);

        // Removing the marker brings the newest version back.
        assert!(delete(&store, "b", "k", Some(&marker)).unwrap().marker);
        assert_eq!(get(&store, "b", "k", None).unwrap().version, v2.version);
        delete(&store, "b", "k", Some(&v2.version)).unwrap();
        assert_eq!(get(&store, "b", "k", None).unwrap().version, v1.version);
        assert!(matches!(
            get(&store, "b", "k", Some("gone")),
            Err(Problem::NoVersion { .. })
        ));

        // Suspended: a write replaces `null`, the others stay.
        set_versioning(&store, "b", Versioning::Suspended).unwrap();
        let again = add(&store, "b", "k", "v3").await;
        assert_eq!(again.version, NO_VERSION);
        assert!(!files::path(&store, &plain.file).exists());
        assert_eq!(
            list_versions(&store, "b", "", "", ("", None), 10)
                .unwrap()
                .versions
                .len(),
            2
        );
    }

    #[tokio::test]
    async fn builds_an_object_from_parts() {
        let store = store();
        create_bucket(&store, "b", "r").unwrap();
        let meta = Meta {
            content_type: "video/mp4".into(),
            ..Meta::default()
        };
        let upload = create_upload(&store, "b", "big", &meta).unwrap();
        put_part(&store, &upload.id, 1, &write(&store, "wrong").await).unwrap();
        let one = write(&store, "hello ").await;
        let two = write(&store, "world").await;
        put_part(&store, &upload.id, 1, &one).unwrap();
        put_part(&store, &upload.id, 2, &two).unwrap();
        assert_eq!(
            parts(&store, &upload.id)
                .unwrap()
                .iter()
                .map(|p| p.size)
                .collect::<Vec<_>>(),
            [6, 5]
        );
        assert_eq!(uploads(&store, "b").unwrap().len(), 1);

        let out_of_order = [(2, two.md5.clone()), (1, one.md5.clone())];
        assert!(matches!(
            complete_upload(&store, &upload.id, &out_of_order).await,
            Err(Problem::BadPart(_))
        ));
        let missing = [(1, one.md5.clone()), (3, String::new())];
        assert!(matches!(
            complete_upload(&store, &upload.id, &missing).await,
            Err(Problem::BadPart(_))
        ));

        let chosen = [(1, format!("\"{}\"", one.md5)), (2, two.md5.clone())];
        let object = complete_upload(&store, &upload.id, &chosen).await.unwrap();
        assert_eq!(object.size, 11);
        assert!(
            object.etag.ends_with("-2") && object.etag.len() == 34,
            "{}",
            object.etag
        );
        assert_eq!(object.meta.content_type, "video/mp4");
        assert_eq!(
            std::fs::read(files::path(&store, &object.file)).unwrap(),
            b"hello world"
        );
        // The upload and its parts are gone.
        assert_eq!(
            upload_gone(&store, &upload.id),
            Problem::NoUpload(upload.id.clone())
        );
        assert!(!files::path(&store, &one.file).exists());

        let abandoned = create_upload(&store, "b", "never", &Meta::default()).unwrap();
        let part = write(&store, "x").await;
        put_part(&store, &abandoned.id, 1, &part).unwrap();
        remove_upload(&store, &abandoned.id).unwrap();
        assert!(!files::path(&store, &part.file).exists());
    }

    fn upload_gone(store: &Store, id: &str) -> Problem {
        upload(store, id).unwrap_err()
    }

    #[test]
    fn keeps_documents_with_a_bucket() {
        let store = store();
        create_bucket(&store, "b", "r").unwrap();
        assert_eq!(config(&store, "b", "cors").unwrap(), None);
        set_config(&store, "b", "cors", Some("<CORSConfiguration/>")).unwrap();
        set_config(&store, "b", "policy", Some("{}")).unwrap();
        assert_eq!(
            config(&store, "b", "cors").unwrap().as_deref(),
            Some("<CORSConfiguration/>")
        );
        set_config(&store, "b", "cors", None).unwrap();
        assert_eq!(config(&store, "b", "cors").unwrap(), None);
        assert_eq!(
            config(&store, "b", "policy").unwrap().as_deref(),
            Some("{}")
        );
        assert!(matches!(
            config(&store, "none", "cors"),
            Err(Problem::NoBucket(_))
        ));
    }
}
