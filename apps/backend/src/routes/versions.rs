//! Version history endpoints and structural diff.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use dp_model::Document;
use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;

use crate::auth::{self, AuthUser};
use crate::authz::{self, Capability};
use crate::error::{ApiError, AppResult};
use crate::models::{Page, VersionSummary};
use crate::pagination::{PageParams, PageRequest};
use crate::routes::files::{fetch_file, fetch_version_document};
use crate::services::{audit, diff};
use crate::state::AppState;
use crate::validation;

/// Named version payload.
#[derive(Debug, Deserialize)]
pub struct CreateVersionRequest {
    /// Human-readable label.
    pub label: String,
}

/// Diff query parameters.
#[derive(Debug, Deserialize)]
pub struct DiffQuery {
    /// Base version id.
    pub from: Option<Uuid>,
    /// Target version id.
    pub to: Option<Uuid>,
}

/// Diff response.
#[derive(Debug, serde::Serialize)]
pub struct DiffResponse {
    /// Base version.
    pub from: VersionSummary,
    /// Target version.
    pub to: VersionSummary,
    /// Structural diff.
    #[serde(flatten)]
    pub diff: diff::DocumentDiff,
}

/// List versions of a file, newest first.
pub async fn list_versions(
    State(state): State<AppState>,
    user: AuthUser,
    Path(file_id): Path<Uuid>,
    Query(params): Query<PageParams>,
) -> AppResult<Json<Page<VersionSummary>>> {
    authz::require_file(&state.db, user.user_id, file_id, Capability::View).await?;
    let page = PageRequest::resolve(params);
    let total: i64 = sqlx::query_scalar("SELECT count(*) FROM file_versions WHERE file_id = $1")
        .bind(file_id)
        .fetch_one(&state.db)
        .await?;
    let rows: Vec<VersionSummary> = sqlx::query_as(
        "SELECT id, file_id, label, kind, parent_version_id, created_by, created_at, op_seq \
         FROM file_versions WHERE file_id = $1 \
         ORDER BY created_at DESC, id DESC LIMIT $2 OFFSET $3",
    )
    .bind(file_id)
    .bind(i64::from(page.limit))
    .bind(page.offset)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(Page::new(rows, page, total)))
}

/// Create a named version from the current head document.
pub async fn create_named_version(
    State(state): State<AppState>,
    user: AuthUser,
    headers: HeaderMap,
    Path(file_id): Path<Uuid>,
    Json(payload): Json<CreateVersionRequest>,
) -> AppResult<Response> {
    authz::require_file(&state.db, user.user_id, file_id, Capability::Edit).await?;
    let label = validation::validate_label(&payload.label).map_err(|message| {
        ApiError::validation_failed(serde_json::json!([{"field": "label", "message": message}]))
    })?;
    let file = fetch_file(&state, file_id).await?;
    let head = file
        .head_version_id
        .ok_or_else(|| ApiError::conflict("this file has no document to snapshot yet"))?;
    let (document, _head_version) = fetch_version_document(&state, head).await?;

    let version: VersionSummary = sqlx::query_as(
        "INSERT INTO file_versions (file_id, label, kind, parent_version_id, snapshot_json, created_by) \
         VALUES ($1, $2, 'named', $3, $4, $5) \
         RETURNING id, file_id, label, kind, parent_version_id, created_by, created_at, op_seq",
    )
    .bind(file_id)
    .bind(&label)
    .bind(file.head_version_id)
    .bind(&document)
    .bind(user.user_id)
    .fetch_one(&state.db)
    .await?;

    audit::record_pool(
        &state.db,
        Some(user.user_id),
        "version.create",
        "file",
        Some(&file_id.to_string()),
        serde_json::json!({"version_id": version.id, "label": label}),
        Some(&auth::client_ip(&headers, None)),
    )
    .await?;
    Ok((StatusCode::CREATED, Json(version)).into_response())
}

/// Restore a snapshot version as a new head version.
pub async fn restore_version(
    State(state): State<AppState>,
    user: AuthUser,
    headers: HeaderMap,
    Path((file_id, version_id)): Path<(Uuid, Uuid)>,
) -> AppResult<Json<VersionSummary>> {
    authz::require_file(&state.db, user.user_id, file_id, Capability::Edit).await?;
    let file = fetch_file(&state, file_id).await?;
    let snapshot: Option<Option<Value>> = sqlx::query_scalar(
        "SELECT snapshot_json FROM file_versions WHERE id = $1 AND file_id = $2",
    )
    .bind(version_id)
    .bind(file_id)
    .fetch_optional(&state.db)
    .await?;
    let snapshot = snapshot.flatten().ok_or_else(|| {
        ApiError::unprocessable(
            "unsupported_version",
            "only snapshot versions can be restored; this version has no snapshot payload",
        )
    })?;
    // Refuse to restore data that no longer parses as a document.
    Document::from_json_value(snapshot.clone()).map_err(|err| {
        ApiError::unprocessable(
            "invalid_document",
            format!("the stored version is not a valid document: {err}"),
        )
    })?;

    let mut tx = state.db.begin().await?;
    let version: VersionSummary = sqlx::query_as(
        "INSERT INTO file_versions (file_id, label, kind, parent_version_id, snapshot_json, created_by) \
         VALUES ($1, '', 'snapshot', $2, $3, $4) \
         RETURNING id, file_id, label, kind, parent_version_id, created_by, created_at, op_seq",
    )
    .bind(file_id)
    .bind(file.head_version_id)
    .bind(&snapshot)
    .bind(user.user_id)
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query("UPDATE files SET head_version_id = $2 WHERE id = $1")
        .bind(file_id)
        .bind(version.id)
        .execute(&mut *tx)
        .await?;
    audit::record(
        &mut *tx,
        Some(user.user_id),
        "version.restore",
        "file",
        Some(&file_id.to_string()),
        serde_json::json!({"restored_version_id": version_id, "new_version_id": version.id}),
        Some(&auth::client_ip(&headers, None)),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(version))
}

/// Compute a structural diff between two snapshot versions.
pub async fn diff_versions(
    State(state): State<AppState>,
    user: AuthUser,
    Path(file_id): Path<Uuid>,
    Query(query): Query<DiffQuery>,
) -> AppResult<Json<DiffResponse>> {
    authz::require_file(&state.db, user.user_id, file_id, Capability::View).await?;
    let from_id = query
        .from
        .ok_or_else(|| ApiError::bad_request_with("missing_parameter", "`from` is required"))?;
    let to_id = query
        .to
        .ok_or_else(|| ApiError::bad_request_with("missing_parameter", "`to` is required"))?;

    let (from_document, from_version) = version_document(&state, file_id, from_id).await?;
    let (to_document, to_version) = version_document(&state, file_id, to_id).await?;
    let diff = diff::diff_documents(&from_document, &to_document)?;
    Ok(Json(DiffResponse {
        from: from_version,
        to: to_version,
        diff,
    }))
}

async fn version_document(
    state: &AppState,
    file_id: Uuid,
    version_id: Uuid,
) -> AppResult<(Document, VersionSummary)> {
    let snapshot: Option<Option<Value>> = sqlx::query_scalar(
        "SELECT snapshot_json FROM file_versions WHERE id = $1 AND file_id = $2",
    )
    .bind(version_id)
    .bind(file_id)
    .fetch_optional(&state.db)
    .await?;
    let snapshot = snapshot.flatten().ok_or_else(|| {
        ApiError::unprocessable(
            "unsupported_version",
            "diffing requires snapshot versions; this version has no snapshot payload",
        )
    })?;
    let document = Document::from_json_value(snapshot).map_err(|err| {
        ApiError::unprocessable(
            "invalid_document",
            format!("the stored version is not a valid document: {err}"),
        )
    })?;
    let version: VersionSummary = sqlx::query_as(
        "SELECT id, file_id, label, kind, parent_version_id, created_by, created_at, op_seq \
         FROM file_versions WHERE id = $1",
    )
    .bind(version_id)
    .fetch_one(&state.db)
    .await?;
    Ok((document, version))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_query_requires_both_versions() {
        let query: DiffQuery = serde_json::from_str("{}").expect("parse");
        assert!(query.from.is_none());
        assert!(query.to.is_none());
    }

    #[test]
    fn diff_query_parses_ids() {
        let id = Uuid::nil();
        let query: DiffQuery = serde_json::from_value(serde_json::json!({
            "from": id,
            "to": Uuid::max(),
        }))
        .expect("parse");
        assert_eq!(query.from, Some(id));
        assert_eq!(query.to, Some(Uuid::max()));
    }
}
