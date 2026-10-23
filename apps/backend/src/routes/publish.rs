//! Publishing endpoints: generate a static export bundle and serve it.

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::HeaderValue;
use axum::http::header::{
    CACHE_CONTROL, CONTENT_SECURITY_POLICY, CONTENT_TYPE, X_CONTENT_TYPE_OPTIONS,
};
use axum::response::Response;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::auth::{self, AuthUser};
use crate::authz::{self, Capability};
use crate::error::{ApiError, AppResult};
use crate::routes::files::{fetch_file, fetch_version_document};
use crate::services::{audit, publish_export};
use crate::state::AppState;
use crate::validation;

/// Publish request.
#[derive(Debug, Default, Deserialize)]
pub struct PublishRequest {
    /// Optional custom slug; derived from the file name when omitted.
    #[serde(default)]
    pub slug: Option<String>,
}

/// Publish response.
#[derive(Debug, Serialize)]
pub struct PublishResponse {
    /// Public slug.
    pub slug: String,
    /// Absolute public URL.
    pub url: String,
    /// Published version.
    pub version_id: Uuid,
    /// Bundle size in bytes.
    pub byte_size: i64,
    /// Bundle content sha256 (hex).
    pub sha256: String,
    /// Publish timestamp.
    pub published_at: DateTime<Utc>,
}

#[derive(Debug, sqlx::FromRow)]
struct PublishedSiteRow {
    version_id: Option<Uuid>,
    slug: String,
    object_key: String,
    byte_size: i64,
    created_at: DateTime<Utc>,
}

/// Publish the current head document as a static site bundle.
pub async fn publish_file(
    State(state): State<AppState>,
    user: AuthUser,
    headers: axum::http::HeaderMap,
    Path(file_id): Path<Uuid>,
    body: Bytes,
) -> AppResult<Json<PublishResponse>> {
    authz::require_file(&state.db, user.user_id, file_id, Capability::Edit).await?;
    let payload: PublishRequest = if body.is_empty() {
        PublishRequest::default()
    } else {
        serde_json::from_slice(&body).map_err(|err| {
            ApiError::bad_request_with("invalid_json", format!("invalid JSON body: {err}"))
        })?
    };

    let file = fetch_file(&state, file_id).await?;
    let head = file
        .head_version_id
        .ok_or_else(|| ApiError::conflict("this file has no document to publish yet"))?;
    let (document_json, version) = fetch_version_document(&state, head).await?;
    let document = dp_model::Document::from_json_value(document_json.clone()).map_err(|err| {
        ApiError::unprocessable(
            "invalid_document",
            format!("the head document cannot be parsed: {err}"),
        )
    })?;

    let base_slug = match payload.slug.as_deref() {
        Some(raw) => validation::validate_slug(raw).map_err(|message| {
            ApiError::validation_failed(serde_json::json!([{"field": "slug", "message": message}]))
        })?,
        None => validation::slugify(&file.name),
    };
    let slug = allocate_slug(&state, file_id, &base_slug).await?;
    let html = publish_export::render_export_html(&document, &document_json, &slug, &file.name)
        .map_err(|err| {
            ApiError::internal_context("rendering the publish bundle", err.to_string())
        })?;
    let bytes = Bytes::from(html.into_bytes());
    let digest = Sha256::digest(&bytes);
    let sha256_hex = hex::encode(digest);
    let object_key = format!("publishes/{slug}/{sha256_hex}.html");
    crate::routes::files::ensure_storage_key(&object_key)?;
    state
        .store
        .put(&object_key, bytes.clone())
        .await
        .map_err(|err| {
            ApiError::internal_context("storing the publish bundle", format!("{err:?}"))
        })?;

    let mut tx = state.db.begin().await?;
    let row: PublishedSiteRow = sqlx::query_as(
        "INSERT INTO published_sites \
            (file_id, version_id, slug, object_key, byte_size, sha256, created_by) \
         VALUES ($1, $2, $3, $4, $5, $6, $7) \
         ON CONFLICT (file_id) DO UPDATE SET \
            version_id = EXCLUDED.version_id, slug = EXCLUDED.slug, \
            object_key = EXCLUDED.object_key, byte_size = EXCLUDED.byte_size, \
            sha256 = EXCLUDED.sha256, created_at = now() \
         RETURNING file_id, version_id, slug, object_key, byte_size, created_at",
    )
    .bind(file_id)
    .bind(version.id)
    .bind(&slug)
    .bind(&object_key)
    .bind(bytes.len() as i64)
    .bind(digest.as_slice())
    .bind(user.user_id)
    .fetch_one(&mut *tx)
    .await?;
    audit::record(
        &mut *tx,
        Some(user.user_id),
        "file.publish",
        "file",
        Some(&file_id.to_string()),
        serde_json::json!({
            "slug": slug,
            "version_id": version.id,
            "byte_size": bytes.len(),
        }),
        Some(&auth::client_ip(&headers, None)),
    )
    .await?;
    tx.commit().await?;

    Ok(Json(PublishResponse {
        url: format!("{}/p/{}", state.config.public_base_url, row.slug),
        slug: row.slug,
        version_id: row.version_id.unwrap_or(version.id),
        byte_size: row.byte_size,
        sha256: sha256_hex,
        published_at: row.created_at,
    }))
}

async fn allocate_slug(state: &AppState, file_id: Uuid, base: &str) -> AppResult<String> {
    // Reuse the file's own existing slug so republishing keeps stable URLs.
    let existing: Option<String> =
        sqlx::query_scalar("SELECT slug FROM published_sites WHERE file_id = $1")
            .bind(file_id)
            .fetch_optional(&state.db)
            .await?;
    if let Some(slug) = existing {
        let owned: Option<Uuid> =
            sqlx::query_scalar("SELECT file_id FROM published_sites WHERE slug = $1")
                .bind(&slug)
                .fetch_optional(&state.db)
                .await?;
        if owned == Some(file_id) {
            return Ok(slug);
        }
    }
    for attempt in 0..8_u32 {
        let candidate = if attempt == 0 {
            base.to_owned()
        } else {
            let suffix = format!("-{:06x}", random_u32()? & 0x00ff_ffff);
            let mut candidate = truncate_slug(base, 63 - suffix.len());
            candidate.push_str(&suffix);
            candidate
        };
        let taken: Option<Uuid> =
            sqlx::query_scalar("SELECT file_id FROM published_sites WHERE slug = $1")
                .bind(&candidate)
                .fetch_optional(&state.db)
                .await?;
        match taken {
            None => return Ok(candidate),
            Some(owner) if owner == file_id => return Ok(candidate),
            Some(_) => continue,
        }
    }
    Err(ApiError::conflict(
        "could not allocate a unique publish slug; pass an explicit `slug`",
    ))
}

fn truncate_slug(slug: &str, max: usize) -> String {
    let mut out: String = slug.chars().take(max).collect();
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() {
        "site".to_owned()
    } else {
        out
    }
}

fn random_u32() -> AppResult<u32> {
    let mut bytes = [0_u8; 4];
    getrandom::fill(&mut bytes)
        .map_err(|err| ApiError::internal(format!("random suffix: {err}")))?;
    Ok(u32::from_be_bytes(bytes))
}

/// Serve a published bundle.
pub async fn serve_published(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> AppResult<Response> {
    let row: Option<PublishedSiteRow> = sqlx::query_as(
        "SELECT file_id, version_id, slug, object_key, byte_size, created_at \
         FROM published_sites WHERE slug = $1",
    )
    .bind(&slug)
    .fetch_optional(&state.db)
    .await?;
    let row = row.ok_or_else(|| {
        ApiError::not_found_with("not_published", "no published site exists for this slug")
    })?;
    let bytes = state
        .store
        .get(&row.object_key)
        .await
        .map_err(|err| {
            ApiError::internal_context("reading the publish bundle", format!("{err:?}"))
        })?
        .ok_or_else(|| {
            ApiError::not_found_with(
                "not_published",
                "the published bundle for this slug is missing from storage",
            )
        })?;
    let mut body = Vec::with_capacity(row.byte_size as usize);
    use futures::StreamExt as _;
    let mut stream = bytes;
    while let Some(chunk) = stream.next().await {
        let chunk =
            chunk.map_err(|err| ApiError::internal_context("reading the publish bundle", err))?;
        body.extend_from_slice(&chunk);
        if body.len() > 64 * 1024 * 1024 {
            return Err(ApiError::internal(
                "the published bundle exceeds the in-memory serving limit",
            ));
        }
    }

    let mut response = Response::new(axum::body::Body::from(body));
    response.headers_mut().insert(
        CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    let headers = response.headers_mut();
    headers.insert(
        CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(
            "sandbox; default-src 'none'; img-src data: blob:; style-src 'unsafe-inline'; font-src data:",
        ),
    );
    headers.insert(X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    headers.insert(
        CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=60"),
    );
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_truncation_keeps_a_valid_slug() {
        let long = "abcdefghijklmnopqrstuvwxyz-abcdefghijklmnopqrstuvwxyz-abcdefghijklmnop";
        let truncated = truncate_slug(long, 10);
        assert!(truncated.chars().count() <= 10);
        assert!(!truncated.ends_with('-'));
        assert!(validation::validate_slug(&truncated).is_ok());
    }

    #[test]
    fn publish_request_defaults() {
        let request: PublishRequest = serde_json::from_str("{}").expect("parse");
        assert!(request.slug.is_none());
    }
}
