//! S3-compatible object store, compiled only with the `s3` feature.
//!
//! Works with AWS S3, MinIO, Cloudflare R2 and other S3-compatible services.
//! Compiled and type-checked in this repository, but not exercised against a
//! live bucket in CI (see the README limitations).

use bytes::Bytes;
use futures::future::BoxFuture;

use super::{ByteStream, ObjectMeta, ObjectStore, StoreError};
use crate::config::S3Config;

use aws_sdk_s3::config::{BehaviorVersion, Credentials, Region};
use aws_sdk_s3::primitives::ByteStream as S3Body;

/// S3-backed object store.
#[derive(Debug, Clone)]
pub struct S3Store {
    client: aws_sdk_s3::Client,
    bucket: String,
    prefix: String,
}

impl S3Store {
    /// Build a client from validated configuration.
    pub fn new(config: &S3Config) -> Result<Self, StoreError> {
        let mut builder = aws_sdk_s3::config::Builder::new()
            .behavior_version(BehaviorVersion::latest())
            .region(Region::new(config.region.clone()));
        if let Some(endpoint) = &config.endpoint {
            builder = builder
                .endpoint_url(endpoint.clone())
                .force_path_style(config.force_path_style);
        }
        if let (Some(access_key_id), Some(secret_access_key)) =
            (&config.access_key_id, &config.secret_access_key)
        {
            builder = builder.credentials_provider(Credentials::new(
                access_key_id.clone(),
                secret_access_key.clone(),
                None,
                None,
                "iconcrafter-static",
            ));
        }
        Ok(Self {
            client: aws_sdk_s3::Client::from_conf(builder.build()),
            bucket: config.bucket.clone(),
            prefix: config.prefix.clone(),
        })
    }

    fn object_key(&self, key: &str) -> Result<String, StoreError> {
        super::validate_key(key)?;
        Ok(format!("{}{}", self.prefix, key))
    }
}

fn backend_error(error: impl std::fmt::Display) -> StoreError {
    StoreError::Backend(error.to_string())
}

impl ObjectStore for S3Store {
    fn kind(&self) -> &'static str {
        "s3"
    }

    fn put(&self, key: &str, data: Bytes) -> BoxFuture<'_, Result<(), StoreError>> {
        let object_key = self.object_key(key);
        Box::pin(async move {
            let object_key = object_key?;
            self.client
                .put_object()
                .bucket(&self.bucket)
                .key(object_key)
                .body(S3Body::from(data))
                .send()
                .await
                .map_err(backend_error)?;
            Ok(())
        })
    }

    fn get(&self, key: &str) -> BoxFuture<'_, Result<Option<ByteStream>, StoreError>> {
        let object_key = self.object_key(key);
        Box::pin(async move {
            let object_key = object_key?;
            let response = match self
                .client
                .get_object()
                .bucket(&self.bucket)
                .key(object_key)
                .send()
                .await
            {
                Ok(response) => response,
                Err(error) => {
                    if error
                        .as_service_error()
                        .is_some_and(|service| service.is_no_such_key())
                    {
                        return Ok(None);
                    }
                    return Err(backend_error(error));
                }
            };
            let reader = tokio_util::io::ReaderStream::new(response.body.into_async_read());
            let stream: ByteStream = Box::pin(reader);
            Ok(Some(stream))
        })
    }

    fn head(&self, key: &str) -> BoxFuture<'_, Result<Option<ObjectMeta>, StoreError>> {
        let object_key = self.object_key(key);
        Box::pin(async move {
            let object_key = object_key?;
            match self
                .client
                .head_object()
                .bucket(&self.bucket)
                .key(object_key)
                .send()
                .await
            {
                Ok(response) => Ok(Some(ObjectMeta {
                    size: response.content_length().unwrap_or(0).max(0) as u64,
                })),
                Err(error) => {
                    if error
                        .as_service_error()
                        .is_some_and(|service| service.is_not_found())
                    {
                        Ok(None)
                    } else {
                        Err(backend_error(error))
                    }
                }
            }
        })
    }

    fn delete(&self, key: &str) -> BoxFuture<'_, Result<bool, StoreError>> {
        let object_key = self.object_key(key);
        Box::pin(async move {
            let object_key = object_key?;
            // S3 delete is idempotent and does not report whether an object
            // existed, so a successful delete is reported as `true`.
            self.client
                .delete_object()
                .bucket(&self.bucket)
                .key(object_key)
                .send()
                .await
                .map_err(backend_error)?;
            Ok(true)
        })
    }
}
