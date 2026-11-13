//! Where data lives: a folder with a SQLite database for records and a
//! `files` folder for large bodies. Without a folder, all of it is in
//! memory and a temporary folder, gone when the program exits.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use rusqlite::Connection;

pub struct Store {
    db: Mutex<Connection>,
    files: PathBuf,
    /// Removed on drop when the store is in memory only.
    temporary: bool,
}

impl Store {
    /// Opens the data in `dir`, creating it; in memory when there's none.
    pub fn open(dir: Option<&Path>) -> Result<Self, String> {
        let (db, files, temporary) = match dir {
            Some(dir) => {
                std::fs::create_dir_all(dir)
                    .map_err(|e| format!("can't create {}: {e}", dir.display()))?;
                let db = Connection::open(dir.join("ferrocloud.db")).map_err(|e| e.to_string())?;
                // Writers don't block readers, and a crash loses nothing
                // that was acknowledged.
                db.pragma_update(None, "journal_mode", "WAL")
                    .map_err(|e| e.to_string())?;
                (db, dir.join("files"), false)
            }
            None => {
                let db = Connection::open_in_memory().map_err(|e| e.to_string())?;
                // One folder per store, even for two opened in the same
                // millisecond (as tests do): each removes its own on drop.
                static OPENED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
                let files = std::env::temp_dir().join(format!(
                    "ferrocloud-{}-{:x}-{}",
                    std::process::id(),
                    crate::clock::now_millis(),
                    OPENED.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                ));
                (db, files, true)
            }
        };
        std::fs::create_dir_all(&files)
            .map_err(|e| format!("can't create {}: {e}", files.display()))?;
        Ok(Self {
            db: Mutex::new(db),
            files,
            temporary,
        })
    }

    /// The database. Held only for the length of a query: never across an
    /// await.
    pub fn db(&self) -> MutexGuard<'_, Connection> {
        self.db
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// The folder for large bodies, such as S3 objects.
    pub fn files(&self) -> &Path {
        &self.files
    }

    /// Deletes everything: every table and every file. Services set their
    /// tables up again afterwards.
    pub fn reset(&self) -> Result<(), String> {
        let db = self.db();
        let tables: Vec<String> = db
            .prepare(
                "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
            )
            .and_then(|mut q| q.query_map([], |row| row.get(0))?.collect())
            .map_err(|e| e.to_string())?;
        for table in tables {
            db.execute_batch(&format!("DROP TABLE \"{}\"", table.replace('"', "\"\"")))
                .map_err(|e| e.to_string())?;
        }
        drop(db);
        if self.files.exists() {
            std::fs::remove_dir_all(&self.files).map_err(|e| e.to_string())?;
        }
        std::fs::create_dir_all(&self.files).map_err(|e| e.to_string())
    }

    /// Removes the temporary folder of an in-memory store. Also done on
    /// drop; for when the program exits without dropping it.
    pub fn clean_up(&self) {
        if self.temporary {
            let _ = std::fs::remove_dir_all(&self.files);
        }
    }
}

impl Drop for Store {
    fn drop(&mut self) {
        self.clean_up();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_data_in_a_folder_and_resets() {
        let dir = std::env::temp_dir().join(format!("ferrocloud-store-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        {
            let store = Store::open(Some(&dir)).unwrap();
            store
                .db()
                .execute_batch("CREATE TABLE t (v TEXT); INSERT INTO t VALUES ('kept')")
                .unwrap();
            std::fs::write(store.files().join("body"), b"x").unwrap();
        }
        // Still there when opened again.
        let store = Store::open(Some(&dir)).unwrap();
        let value: String = store
            .db()
            .query_row("SELECT v FROM t", [], |r| r.get(0))
            .unwrap();
        assert_eq!(value, "kept");
        assert!(store.files().join("body").exists());

        store.reset().unwrap();
        assert!(store.db().prepare("SELECT v FROM t").is_err());
        assert!(store.files().exists() && !store.files().join("body").exists());
        drop(store);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn in_memory_leaves_nothing_behind() {
        let store = Store::open(None).unwrap();
        let files = store.files().to_owned();
        assert!(files.exists());
        store.db().execute_batch("CREATE TABLE t (v TEXT)").unwrap();
        drop(store);
        assert!(!files.exists());
    }
}
