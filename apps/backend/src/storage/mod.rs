//! Object storage abstraction.
//!
//! All objects are stored under server-generated, content-addressed keys; client
//! supplied file names are never used as keys. [`LocalFsStore`] is always
//! available, [`S3Store`] is compiled behind the `s3` feature.
//!
//! The trait uses boxed futures instead of an `async_trait` dependency so it
//! stays dyn-compatible without extra crates.

pub mod local;
pub mod sniff;
#[cfg(feature = "s3")]
pub mod s3;

use std::sync::Arc;

use bytes::Bytes;
use futures::future::BoxFuture;
use futures::stream::BoxStream;

use crate::config::Config;

/// Streaming body returned by [`ObjectStore::get`].
pub type ByteStream = BoxStream<'static, Result<Bytes, std::io::Error>>;

/// Metadata for a stored object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObjectMeta {
    /// Object size in bytes.
    pub size: u64,
}

/// Object storage failures.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// The key is not a valid server-generated object key.
    #[error("invalid object key: {0}")]
    InvalidKey(String),
    /// Local filesystem failure.
    #[error("object storage I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// Remote backend failure (S3 and compatible services).
    #[error("object storage backend error: {0}")]
    Backend(String),
}

impl StoreError {
    /// True when the failure means the object does not exist.
    pub fn is_not_found(&self) -> bool {
        matches!(self, Self::Io(error) if error.kind() == std::io::ErrorKind::NotFound)
    }
}

/// Minimal async object store.
pub trait ObjectStore: Send + Sync {
    /// Stable backend name used in logs (`local` or `s3`).
    fn kind(&self) -> &'static str;
    /// Store `data` under `key`, replacing nothing: keys are content-addressed.
    fn put(&self, key: &str, data: Bytes) -> BoxFuture<'_, Result<(), StoreError>>;
    /// Fetch an object; `None` when it does not exist.
    fn get(&self, key: &str) -> BoxFuture<'_, Result<Option<ByteStream>, StoreError>>;
    /// Metadata for an object; `None` when it does not exist.
    fn head(&self, key: &str) -> BoxFuture<'_, Result<Option<ObjectMeta>, StoreError>>;
    /// Delete an object. Returns `true` when an object was removed.
    fn delete(&self, key: &str) -> BoxFuture<'_, Result<bool, StoreError>>;
}

/// Maximum accepted key length.
pub const MAX_KEY_LENGTH: usize = 512;

/// Validate a server-generated object key.
///
/// Keys are relative, forward-slash separated paths built from
/// `[A-Za-z0-9._-]` segments. Absolute paths, `..`, `.`, backslashes, drive
/// letters and empty segments are rejected so a bug elsewhere cannot escape the
/// storage root.
pub fn validate_key(key: &str) -> Result<(), StoreError> {
    let invalid = |reason: &str| StoreError::InvalidKey(format!("{reason}: {key:?}"));
    if key.is_empty() {
        return Err(invalid("key is empty"));
    }
    if key.len() > MAX_KEY_LENGTH {
        return Err(invalid("key is too long"));
    }
    if key.starts_with('/') {
        return Err(invalid("key must be relative"));
    }
    if key.contains('\\') {
        return Err(invalid("key must use `/` separators"));
    }
    if key.contains('\0') {
        return Err(invalid("key must not contain NUL"));
    }
    for segment in key.split('/') {
        if segment.is_empty() {
            return Err(invalid("key must not contain empty segments"));
        }
        if segment == "." || segment == ".." {
            return Err(invalid("key must not contain `.` or `..` segments"));
        }
        if !segment
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        {
            return Err(invalid("key contains an unsupported character"));
        }
        if is_windows_reserved(segment) {
            return Err(invalid("key contains a reserved Windows device name"));
        }
    }
    Ok(())
}

fn is_windows_reserved(segment: &str) -> bool {
    let stem = segment.split('.').next().unwrap_or(segment);
    let upper = stem.to_ascii_uppercase();
    matches!(
        upper.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "COM1" | "COM2" | "COM3" | "COM4" | "COM5" | "COM6" | "COM7"
            | "COM8" | "COM9" | "LPT1" | "LPT2" | "LPT3" | "LPT4" | "LPT5" | "LPT6" | "LPT7" | "LPT8"
            | "LPT9"
    )
}

/// Build the configured object store.
pub fn build_store(config: &Config) -> Result<Arc<dyn ObjectStore>, StoreError> {
    match config.storage_kind {
        crate::config::StorageKind::Local => {
            let store = local::LocalFsStore::new(&config.storage_local_dir)?;
            Ok(Arc::new(store))
        }
        crate::config::StorageKind::S3 => {
            #[cfg(feature = "s3")]
            {
                let settings = config.s3.as_ref().ok_or_else(|| {
                    StoreError::Backend("STORAGE_KIND=s3 without S3 settings".to_owned())
                })?;
                let store = s3::S3Store::new(settings)?;
                Ok(Arc::new(store))
            }
            #[cfg(not(feature = "s3"))]
            {
                Err(StoreError::Backend(
                    "STORAGE_KIND=s3 requires the `s3` cargo feature".to_owned(),
                ))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_keys_are_accepted() {
        for key in [
            "assets/abc.png",
            "publishes/my-site/index-1234abcd.html",
            "a",
            "a/b/c.txt",
            "file.with.dots.svg",
        ] {
            assert!(validate_key(key).is_ok(), "{key} should be valid");
        }
    }

    #[test]
    fn traversal_keys_are_rejected() {
        for key in [
            "",
            "/etc/passwd",
            "../secret",
            "a/../../b",
            "a/./b",
            "a//b",
            "assets\\image.png",
            "C:/windows/system32",
            "con",
            "a/NUL.png",
            "bad key.png",
            "bad:key",
            "bad\0key",
        ] {
            assert!(validate_key(key).is_err(), "{key} should be invalid");
        }
    }

    #[test]
    fn overly_long_keys_are_rejected() {
        let key = format!("assets/{}", "a".repeat(MAX_KEY_LENGTH));
        assert!(validate_key(&key).is_err());
    }
}
