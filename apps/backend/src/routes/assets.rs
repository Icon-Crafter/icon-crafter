//! Asset upload/download endpoints with magic-byte sniffing and content addressing.

use axum::Json;
use axum::body::Body;
use axum::extract::multipart::Multipart;
use axum::extract::{Path, Query, State};
use axum::http::header::{
    CACHE_CONTROL, CONTENT_DISPOSITION, CONTENT_SECURITY_POLICY, CONTENT_TYPE,
    X_CONTENT_TYPE_OPTIONS,
};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use bytes::Bytes;
use serde::Serialize;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::auth::{self, AuthUser};
use crate::authz::{self, Capability};
use crate::error::{ApiError, AppResult};
use crate::models::{AssetView, Page};
use crate::pagination::{PageParams, PageRequest};
use crate::services::audit;
use crate::state::AppState;
use crate::storage::sniff::{SniffedKind, sniff};

/// Upload response.
#[derive(Debug, Serialize)]
pub struct AssetUploadResponse {
    /// Stored asset metadata.
    pub asset: AssetView,
    /// Whether identical content already existed (deduplicated upload).
    pub deduplicated: bool,
    /// Absolute download URL.
    pub url: String,
}

/// List assets attached to a file.
pub async fn list_assets(
    State(state): State<AppState>,
    user: AuthUser,
    Path(file_id): Path<Uuid>,
    Query(params): Query<PageParams>,
) -> AppResult<Json<Page<AssetView>>> {
    authz::require_file(&state.db, user.user_id, file_id, Capability::View).await?;
    let page = PageRequest::resolve(params);
    let total: i64 = sqlx::query_scalar("SELECT count(*) FROM file_assets WHERE file_id = $1")
        .bind(file_id)
        .fetch_one(&state.db)
        .await?;
    let rows: Vec<AssetView> = sqlx::query_as(
        "SELECT a.id, a.file_id, a.kind, encode(a.sha256, 'hex') AS sha256, a.mime, \
                a.byte_size, a.original_name, a.created_at \
         FROM assets a JOIN file_assets fa ON fa.asset_id = a.id \
         WHERE fa.file_id = $1 \
         ORDER BY a.created_at DESC, a.id DESC LIMIT $2 OFFSET $3",
    )
    .bind(file_id)
    .bind(i64::from(page.limit))
    .bind(page.offset)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(Page::new(rows, page, total)))
}

/// Upload an asset to a file.
pub async fn upload_asset(
    State(state): State<AppState>,
    user: AuthUser,
    headers: HeaderMap,
    Path(file_id): Path<Uuid>,
    mut multipart: Multipart,
) -> AppResult<Response> {
    authz::require_file(&state.db, user.user_id, file_id, Capability::Edit).await?;
    let limit = state.config.max_upload_bytes;
    let (data, original_name) = read_upload(&mut multipart, limit).await?;
    if data.is_empty() {
        return Err(ApiError::unprocessable(
            "empty_upload",
            "the uploaded file is empty",
        ));
    }
    let kind: SniffedKind = sniff(&data).ok_or_else(|| {
        ApiError::unprocessable(
            "unsupported_media_type",
            "unsupported file type; expected PNG, JPEG, WebP, SVG, TTF, OTF or WOFF2",
        )
    })?;
    let digest = Sha256::digest(&data);
    let sha256_hex = hex::encode(digest);
    let storage_key = format!("assets/{sha256_hex}.{}", kind.extension());
    crate::routes::files::ensure_storage_key(&storage_key)?;
    state
        .store
        .put(&storage_key, Bytes::from(data.clone()))
        .await
        .map_err(|err| ApiError::internal_context("storing asset bytes", format!("{err:?}")))?;

    let existing: Option<AssetView> = sqlx::query_as(
        "SELECT id, file_id, kind, encode(sha256, 'hex') AS sha256, mime, byte_size, \
                original_name, created_at FROM assets WHERE sha256 = $1",
    )
    .bind(digest.as_slice())
    .fetch_optional(&state.db)
    .await?;
    let content_existed = existing.is_some();

    let mut tx = state.db.begin().await?;
    let asset = match existing {
        Some(asset) => asset,
        None => {
            sqlx::query_as(
                "INSERT INTO assets (file_id, owner_id, kind, sha256, mime, byte_size, storage_key, original_name) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8) \
                 RETURNING id, file_id, kind, encode(sha256, 'hex') AS sha256, mime, byte_size, \
                           original_name, created_at",
            )
            .bind(file_id)
            .bind(user.user_id)
            .bind(kind.asset_kind())
            .bind(digest.as_slice())
            .bind(kind.mime())
            .bind(data.len() as i64)
            .bind(&storage_key)
            .bind(&original_name)
            .fetch_one(&mut *tx)
            .await?
        }
    };
    let linked = sqlx::query(
        "INSERT INTO file_assets (file_id, asset_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
    )
    .bind(file_id)
    .bind(asset.id)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    let deduplicated = content_existed || linked == 0;
    audit::record(
        &mut *tx,
        Some(user.user_id),
        "asset.upload",
        "asset",
        Some(&asset.id.to_string()),
        serde_json::json!({
            "file_id": file_id,
            "sha256": sha256_hex,
            "byte_size": data.len(),
            "mime": kind.mime(),
            "deduplicated": deduplicated,
        }),
        Some(&auth::client_ip(&headers, None)),
    )
    .await?;
    tx.commit().await?;

    let url = format!(
        "{}/api/files/{file_id}/assets/{}",
        state.config.public_base_url, asset.id
    );
    Ok((
        StatusCode::CREATED,
        Json(AssetUploadResponse {
            asset,
            deduplicated,
            url,
        }),
    )
        .into_response())
}

/// Stream asset bytes.
pub async fn get_asset(
    State(state): State<AppState>,
    user: AuthUser,
    Path((file_id, asset_id)): Path<(Uuid, Uuid)>,
) -> AppResult<Response> {
    authz::require_file(&state.db, user.user_id, file_id, Capability::View).await?;
    let row: Option<(String, String, i64)> = sqlx::query_as(
        "SELECT a.mime, a.storage_key, a.byte_size FROM assets a \
         JOIN file_assets fa ON fa.asset_id = a.id \
         WHERE a.id = $1 AND fa.file_id = $2",
    )
    .bind(asset_id)
    .bind(file_id)
    .fetch_optional(&state.db)
    .await?;
    let (mime, storage_key, byte_size) =
        row.ok_or_else(|| ApiError::not_found("asset not found on this file"))?;
    let stream = state
        .store
        .get(&storage_key)
        .await
        .map_err(|err| ApiError::internal_context("reading asset bytes", format!("{err:?}")))?
        .ok_or_else(|| {
            ApiError::not_found("the asset metadata exists but its bytes are missing from storage")
        })?;

    let mut response = Body::from_stream(stream).into_response();
    let headers = response.headers_mut();
    let content_type = HeaderValue::from_str(&mime)
        .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream"));
    headers.insert(CONTENT_TYPE, content_type);
    headers.insert(
        CACHE_CONTROL,
        HeaderValue::from_static("private, max-age=31536000, immutable"),
    );
    headers.insert(X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    if mime == "image/svg+xml" {
        headers.insert(
            CONTENT_SECURITY_POLICY,
            HeaderValue::from_static("sandbox; default-src 'none'; style-src 'unsafe-inline'"),
        );
        headers.insert(
            CONTENT_DISPOSITION,
            HeaderValue::from_static("inline; filename=\"asset.svg\""),
        );
    }
    headers.insert(
        axum::http::header::CONTENT_LENGTH,
        HeaderValue::from_str(&byte_size.to_string())
            .unwrap_or_else(|_| HeaderValue::from_static("0")),
    );
    Ok(response)
}

/// Remove an asset from a file, deleting the bytes when no file references them.
pub async fn delete_asset(
    State(state): State<AppState>,
    user: AuthUser,
    headers: HeaderMap,
    Path((file_id, asset_id)): Path<(Uuid, Uuid)>,
) -> AppResult<Response> {
    authz::require_file(&state.db, user.user_id, file_id, Capability::Edit).await?;
    let storage_key: Option<String> = sqlx::query_scalar(
        "SELECT a.storage_key FROM assets a JOIN file_assets fa ON fa.asset_id = a.id \
         WHERE a.id = $1 AND fa.file_id = $2",
    )
    .bind(asset_id)
    .bind(file_id)
    .fetch_optional(&state.db)
    .await?;
    let storage_key =
        storage_key.ok_or_else(|| ApiError::not_found("asset not found on this file"))?;

    let mut tx = state.db.begin().await?;
    sqlx::query("DELETE FROM file_assets WHERE file_id = $1 AND asset_id = $2")
        .bind(file_id)
        .bind(asset_id)
        .execute(&mut *tx)
        .await?;
    let remaining: i64 = sqlx::query_scalar("SELECT count(*) FROM file_assets WHERE asset_id = $1")
        .bind(asset_id)
        .fetch_one(&mut *tx)
        .await?;
    let mut removed_row = false;
    if remaining == 0 {
        sqlx::query("DELETE FROM assets WHERE id = $1 AND file_id = $2")
            .bind(asset_id)
            .bind(file_id)
            .execute(&mut *tx)
            .await?;
        removed_row = true;
    }
    audit::record(
        &mut *tx,
        Some(user.user_id),
        "asset.delete",
        "asset",
        Some(&asset_id.to_string()),
        serde_json::json!({"file_id": file_id, "bytes_removed": removed_row}),
        Some(&auth::client_ip(&headers, None)),
    )
    .await?;
    tx.commit().await?;

    if removed_row && let Err(error) = state.store.delete(&storage_key).await {
        tracing::warn!(
            asset_id = %asset_id,
            key = %storage_key,
            error = format!("{error:?}"),
            "failed to delete asset bytes after removing the row"
        );
    }
    Ok(StatusCode::NO_CONTENT.into_response())
}

async fn read_upload(
    multipart: &mut Multipart,
    limit: usize,
) -> AppResult<(Vec<u8>, Option<String>)> {
    while let Some(mut field) = multipart
        .next_field()
        .await
        .map_err(|err| ApiError::bad_request_with("malformed_multipart", err.to_string()))?
    {
        let name = field.name().map(str::to_owned);
        let file_name = field.file_name().map(str::to_owned);
        let is_file = file_name.is_some() || name.as_deref() == Some("file");
        if !is_file {
            continue;
        }
        let mut data = Vec::new();
        while let Some(chunk) = field
            .chunk()
            .await
            .map_err(|err| ApiError::bad_request_with("malformed_multipart", err.to_string()))?
        {
            if data.len() + chunk.len() > limit {
                return Err(ApiError::payload_too_large(
                    format!("uploads must be {limit} bytes or smaller"),
                    limit as u64,
                ));
            }
            data.extend_from_slice(&chunk);
        }
        return Ok((data, sanitize_original_name(file_name.as_deref())));
    }
    Err(ApiError::bad_request(
        "the request must contain a multipart file field",
    ))
}

fn sanitize_original_name(raw: Option<&str>) -> Option<String> {
    let raw = raw?;
    let cleaned: String = raw
        .chars()
        .filter(|ch| !ch.is_control() && *ch != '\\' && *ch != '/')
        .take(255)
        .collect();
    let cleaned = cleaned.trim();
    if cleaned.is_empty() {
        None
    } else {
        Some(cleaned.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn original_names_are_sanitized() {
        assert_eq!(
            sanitize_original_name(Some("../../etc/passwd")),
            Some("....etcpasswd".to_owned())
        );
        assert_eq!(
            sanitize_original_name(Some("icon.svg")),
            Some("icon.svg".to_owned())
        );
        assert_eq!(sanitize_original_name(Some("   ")), None);
        assert_eq!(sanitize_original_name(None), None);
    }

    #[test]
    fn long_names_are_truncated() {
        let long = "a".repeat(400);
        let name = sanitize_original_name(Some(&long)).expect("name");
        assert_eq!(name.chars().count(), 255);
    }
}
