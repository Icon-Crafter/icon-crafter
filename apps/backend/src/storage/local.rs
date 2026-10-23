//! Local filesystem object store.
//!
//! Writes go to a temporary file in the destination directory and are moved into
//! place with an atomic rename, so a crashed or interrupted request can never
//! leave a partially written object behind. The root directory is canonicalized
//! at construction and every resolved path is checked to stay inside it.

use std::path::{Path, PathBuf};

use bytes::Bytes;
use futures::future::BoxFuture;

use super::{validate_key, ByteStream, ObjectMeta, ObjectStore, StoreError};

/// Object store backed by a local directory.
#[derive(Debug, Clone)]
pub struct LocalFsStore {
    root: PathBuf,
}

impl LocalFsStore {
    /// Create the store, ensuring the root directory exists.
    pub fn new(root: impl AsRef<Path>) -> Result<Self, StoreError> {
        let root = root.as_ref();
        std::fs::create_dir_all(root)?;
        let root = std::fs::canonicalize(root)?;
        Ok(Self { root })
    }

    /// Root directory of this store.
    pub fn root(&self) -> &Path {
        &self.root
    }

    fn resolve(&self, key: &str) -> Result<PathBuf, StoreError> {
        validate_key(key)?;
        let relative = key.replace('/', std::path::MAIN_SEPARATOR_STR);
        let path = self.root.join(relative);
        if !path.starts_with(&self.root) {
            return Err(StoreError::InvalidKey(format!(
                "resolved path escapes the storage root: {key:?}"
            )));
        }
        Ok(path)
    }
}

fn random_suffix() -> String {
    let mut bytes = [0_u8; 8];
    match getrandom::fill(&mut bytes) {
        Ok(()) => hex::encode(bytes),
        // A predictable temp name is still safe: the final rename is what
        // publishes the object, and content-addressed keys never overwrite.
        Err(_) => format!("{:x}", std::process::id()),
    }
}

impl ObjectStore for LocalFsStore {
    fn kind(&self) -> &'static str {
        "local"
    }

    fn put(&self, key: &str, data: Bytes) -> BoxFuture<'_, Result<(), StoreError>> {
        let path = match self.resolve(key) {
            Ok(path) => path,
            Err(error) => return Box::pin(async move { Err(error) }),
        };
        Box::pin(async move {
            if tokio::fs::try_exists(&path).await? {
                // Content-addressed keys: identical bytes are already stored.
                return Ok(());
            }
            let parent = path
                .parent()
                .ok_or_else(|| StoreError::InvalidKey(format!("key has no parent: {key:?}")))?;
            tokio::fs::create_dir_all(parent).await?;
            let file_name = path
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(|| StoreError::InvalidKey(format!("key has no file name: {key:?}")))?;
            let temp = parent.join(format!(".{file_name}.tmp-{}", random_suffix()));
            let write_result = async {
                tokio::fs::write(&temp, &data).await?;
                if let Ok(file) = tokio::fs::File::open(&temp).await {
                    // Best effort durability; a failure here should not fail the
                    // request on filesystems that do not support syncing.
                    let _ = file.sync_all().await;
                }
                match tokio::fs::rename(&temp, &path).await {
                    Ok(()) => Ok(()),
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
                    Err(error) => Err(error),
                }
            }
            .await;
            if let Err(error) = write_result {
                let _ = tokio::fs::remove_file(&temp).await;
                return Err(StoreError::Io(error));
            }
            Ok(())
        })
    }

    fn get(&self, key: &str) -> BoxFuture<'_, Result<Option<ByteStream>, StoreError>> {
        let path = match self.resolve(key) {
            Ok(path) => path,
            Err(error) => return Box::pin(async move { Err(error) }),
        };
        Box::pin(async move {
            match tokio::fs::File::open(&path).await {
                Ok(file) => {
                    let stream = tokio_util::io::ReaderStream::new(file);
                    Ok(Some(Box::pin(stream)))
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(error) => Err(StoreError::Io(error)),
            }
        })
    }

    fn head(&self, key: &str) -> BoxFuture<'_, Result<Option<ObjectMeta>, StoreError>> {
        let path = match self.resolve(key) {
            Ok(path) => path,
            Err(error) => return Box::pin(async move { Err(error) }),
        };
        Box::pin(async move {
            match tokio::fs::metadata(&path).await {
                Ok(metadata) if metadata.is_file() => Ok(Some(ObjectMeta {
                    size: metadata.len(),
                })),
                Ok(_) => Ok(None),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(error) => Err(StoreError::Io(error)),
            }
        })
    }

    fn delete(&self, key: &str) -> BoxFuture<'_, Result<bool, StoreError>> {
        let path = match self.resolve(key) {
            Ok(path) => path,
            Err(error) => return Box::pin(async move { Err(error) }),
        };
        Box::pin(async move {
            match tokio::fs::remove_file(&path).await {
                Ok(()) => Ok(true),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
                Err(error) => Err(StoreError::Io(error)),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, LocalFsStore) {
        let dir = tempfile::tempdir().expect("temp dir");
        let store = LocalFsStore::new(dir.path()).expect("store");
        (dir, store)
    }

    #[test]
    fn put_get_head_delete_round_trip() {
        let (_dir, store) = store();
        let runtime = tokio::runtime::Runtime::new().expect("runtime");
        runtime.block_on(async {
            assert_eq!(store.head("assets/a.png").await.expect("head"), None);
            store
                .put("assets/a.png", Bytes::from_static(b"hello"))
                .await
                .expect("put");
            let meta = store.head("assets/a.png").await.expect("head").expect("some");
            assert_eq!(meta.size, 5);
            // Second put of the same content-addressed key is a no-op.
            store
                .put("assets/a.png", Bytes::from_static(b"hello"))
                .await
                .expect("put again");
            let stream = store.get("assets/a.png").await.expect("get").expect("some");
            use futures::StreamExt;
            let collected: Vec<u8> = stream
                .filter_map(|chunk| async move { chunk.ok() })
                .flat_map(|bytes| futures::stream::iter(bytes.to_vec()))
                .collect()
                .await;
            assert_eq!(collected, b"hello");
            assert!(store.delete("assets/a.png").await.expect("delete"));
            assert!(!store.delete("assets/a.png").await.expect("delete again"));
        });
    }

    #[test]
    fn traversal_keys_do_not_touch_the_filesystem() {
        let (dir, store) = store();
        let runtime = tokio::runtime::Runtime::new().expect("runtime");
        runtime.block_on(async {
            let error = store
                .put("../escape.txt", Bytes::from_static(b"x"))
                .await
                .expect_err("must be rejected");
            assert!(matches!(error, StoreError::InvalidKey(_)));
            assert!(!dir.path().parent().unwrap().join("escape.txt").exists());
            assert_eq!(store.get("../../etc/passwd").await.err().map(|e| matches!(e, StoreError::InvalidKey(_))), Some(true));
        });
    }

    #[test]
    fn missing_objects_return_none() {
        let (_dir, store) = store();
        let runtime = tokio::runtime::Runtime::new().expect("runtime");
        runtime.block_on(async {
            assert!(store.get("assets/missing.png").await.expect("get").is_none());
            assert!(store.head("assets/missing.png").await.expect("head").is_none());
        });
    }
}
