//! IAM's records: each role, user, policy and instance profile is a JSON
//! document in one table, by kind and name.

use rusqlite::OptionalExtension;
use serde_json::Value;

use crate::protocol::Fault;
use crate::store::Store;

/// The kinds of record.
pub const ROLE: &str = "role";
pub const USER: &str = "user";
pub const POLICY: &str = "policy";
pub const PROFILE: &str = "instance-profile";
pub const ACCESS_KEY: &str = "access-key";

pub fn init(store: &Store) -> Result<(), String> {
    store
        .db()
        .execute_batch(
            "CREATE TABLE IF NOT EXISTS iam (
                kind TEXT NOT NULL,
                name TEXT NOT NULL,
                doc  TEXT NOT NULL,
                PRIMARY KEY (kind, name)
            )",
        )
        .map_err(|e| e.to_string())
}

fn failed(e: rusqlite::Error) -> Fault {
    Fault::internal(e.to_string())
}

pub fn find(store: &Store, kind: &str, name: &str) -> Result<Option<Value>, Fault> {
    let doc: Option<String> = store
        .db()
        .query_row(
            "SELECT doc FROM iam WHERE kind = ?1 AND name = ?2",
            [kind, name],
            |row| row.get(0),
        )
        .optional()
        .map_err(failed)?;
    Ok(doc.and_then(|d| serde_json::from_str(&d).ok()))
}

/// The record, or IAM's "no such entity" error.
pub fn get(store: &Store, kind: &str, name: &str) -> Result<Value, Fault> {
    find(store, kind, name)?.ok_or_else(|| no_such(kind, name))
}

pub fn put(store: &Store, kind: &str, name: &str, doc: &Value) -> Result<(), Fault> {
    store
        .db()
        .execute(
            "INSERT INTO iam (kind, name, doc) VALUES (?1, ?2, ?3)
             ON CONFLICT (kind, name) DO UPDATE SET doc = excluded.doc",
            [kind, name, &doc.to_string()],
        )
        .map(drop)
        .map_err(failed)
}

/// Adds a record that mustn't be there yet.
pub fn create(store: &Store, kind: &str, name: &str, doc: &Value) -> Result<(), Fault> {
    if find(store, kind, name)?.is_some() {
        return Err(Fault::new(
            409,
            "EntityAlreadyExists",
            format!("{} with name {name} already exists.", title(kind)),
        ));
    }
    put(store, kind, name, doc)
}

pub fn delete(store: &Store, kind: &str, name: &str) -> Result<(), Fault> {
    let removed = store
        .db()
        .execute(
            "DELETE FROM iam WHERE kind = ?1 AND name = ?2",
            [kind, name],
        )
        .map_err(failed)?;
    if removed == 0 {
        return Err(no_such(kind, name));
    }
    Ok(())
}

/// Every record of a kind, by name.
pub fn list(store: &Store, kind: &str) -> Result<Vec<Value>, Fault> {
    let db = store.db();
    let mut query = db
        .prepare("SELECT doc FROM iam WHERE kind = ?1 ORDER BY name")
        .map_err(failed)?;
    let docs = query
        .query_map([kind], |row| row.get::<_, String>(0))
        .and_then(Iterator::collect::<Result<Vec<_>, _>>)
        .map_err(failed)?;
    Ok(docs
        .iter()
        .filter_map(|d| serde_json::from_str(d).ok())
        .collect())
}

pub fn no_such(kind: &str, name: &str) -> Fault {
    Fault::new(
        404,
        "NoSuchEntity",
        format!(
            "The {} with name {name} cannot be found.",
            kind.replace('-', " ")
        ),
    )
}

fn title(kind: &str) -> &'static str {
    match kind {
        ROLE => "Role",
        USER => "User",
        POLICY => "A policy",
        PROFILE => "Instance Profile",
        _ => "Entity",
    }
}
