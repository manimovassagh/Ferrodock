//! Objects' bytes: one file each under the store's folder, written as
//! they arrive and hashed on the way, so no object is ever held in memory.

use std::path::PathBuf;

use tokio::io::AsyncWriteExt;

use super::md5::{Md5, hex};
use crate::store::Store;

/// The folder under the store's files that objects are in.
const DIR: &str = "objects";

/// Where the file with this ID is. IDs are spread over subfolders by
/// their first two characters, so no folder gets huge.
pub fn path(store: &Store, id: &str) -> PathBuf {
    let shard = id.get(..2).unwrap_or("00");
    store.files().join(DIR).join(shard).join(id)
}

/// Removes a file. One that's already gone is fine.
pub fn remove(store: &Store, id: &str) {
    if !id.is_empty() {
        let _ = std::fs::remove_file(path(store, id));
    }
}

/// A file that was written: where, how long, and its MD5.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Written {
    /// The file's ID, for [`path`].
    pub file: String,
    pub size: u64,
    /// The MD5 of its bytes, in hex.
    pub md5: String,
}

/// A file being written. Dropped before [`Writer::finish`], what was
/// written is removed.
pub struct Writer {
    file: Option<tokio::fs::File>,
    path: PathBuf,
    id: String,
    md5: Md5,
    size: u64,
}

impl Writer {
    pub async fn create(store: &Store) -> Result<Self, String> {
        let id = crate::sts::random("", 32).to_lowercase();
        let path = path(store, &id);
        if let Some(dir) = path.parent() {
            tokio::fs::create_dir_all(dir)
                .await
                .map_err(|e| e.to_string())?;
        }
        let file = tokio::fs::File::create(&path)
            .await
            .map_err(|e| e.to_string())?;
        Ok(Self {
            file: Some(file),
            path,
            id,
            md5: Md5::default(),
            size: 0,
        })
    }

    pub async fn write(&mut self, bytes: &[u8]) -> Result<(), String> {
        let file = self.file.as_mut().ok_or("the file is closed")?;
        file.write_all(bytes).await.map_err(|e| e.to_string())?;
        self.md5.update(bytes);
        self.size += bytes.len() as u64;
        Ok(())
    }

    /// How much has been written so far.
    pub fn size(&self) -> u64 {
        self.size
    }

    pub async fn finish(mut self) -> Result<Written, String> {
        let mut file = self.file.take().ok_or("the file is closed")?;
        file.flush().await.map_err(|e| e.to_string())?;
        drop(file);
        Ok(Written {
            file: std::mem::take(&mut self.id),
            size: self.size,
            md5: hex(&std::mem::take(&mut self.md5).finish()),
        })
    }
}

impl Drop for Writer {
    fn drop(&mut self) {
        // Not finished: an upload that broke off, or was refused.
        if self.file.take().is_some() {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

/// Writes the files with these IDs one after another into a new file.
pub async fn join(store: &Store, parts: &[String]) -> Result<Written, String> {
    let mut writer = Writer::create(store).await?;
    let mut buffer = vec![0u8; 256 * 1024];
    for part in parts {
        use tokio::io::AsyncReadExt;
        let mut source = tokio::fs::File::open(path(store, part))
            .await
            .map_err(|e| format!("open part {part}: {e}"))?;
        loop {
            let n = source.read(&mut buffer).await.map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            writer.write(&buffer[..n]).await?;
        }
    }
    writer.finish().await
}

/// Copies `length` bytes of a file from `start` into a new file.
pub async fn copy(store: &Store, id: &str, start: u64, length: u64) -> Result<Written, String> {
    use tokio::io::{AsyncReadExt, AsyncSeekExt};
    let mut writer = Writer::create(store).await?;
    let mut source = tokio::fs::File::open(path(store, id))
        .await
        .map_err(|e| e.to_string())?;
    source
        .seek(std::io::SeekFrom::Start(start))
        .await
        .map_err(|e| e.to_string())?;
    let mut buffer = vec![0u8; 256 * 1024];
    let mut left = length;
    while left > 0 {
        let want = left.min(buffer.len() as u64) as usize;
        let n = source
            .read(&mut buffer[..want])
            .await
            .map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("the file is shorter than its record says".into());
        }
        writer.write(&buffer[..n]).await?;
        left -= n as u64;
    }
    writer.finish().await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn writes_hashes_joins_and_copies() {
        let store = Store::open(None).unwrap();
        let mut writer = Writer::create(&store).await.unwrap();
        writer.write(b"hello ").await.unwrap();
        writer.write(b"world").await.unwrap();
        let first = writer.finish().await.unwrap();
        assert_eq!(first.size, 11);
        assert_eq!(first.md5, "5eb63bbbe01eeed093cb22bb8f5acdc3");
        assert_eq!(
            std::fs::read(path(&store, &first.file)).unwrap(),
            b"hello world"
        );

        let joined = join(&store, &[first.file.clone(), first.file.clone()])
            .await
            .unwrap();
        assert_eq!(
            std::fs::read(path(&store, &joined.file)).unwrap(),
            b"hello worldhello world"
        );
        let part = copy(&store, &joined.file, 6, 5).await.unwrap();
        assert_eq!(std::fs::read(path(&store, &part.file)).unwrap(), b"world");
        assert!(copy(&store, &first.file, 6, 50).await.is_err());

        remove(&store, &first.file);
        remove(&store, &first.file);
        assert!(!path(&store, &first.file).exists());
    }

    #[tokio::test]
    async fn an_unfinished_file_is_removed() {
        let store = Store::open(None).unwrap();
        let mut writer = Writer::create(&store).await.unwrap();
        writer.write(b"half").await.unwrap();
        let path = writer.path.clone();
        assert!(path.exists());
        drop(writer);
        assert!(!path.exists());
    }
}
