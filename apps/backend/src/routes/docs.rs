//! Document read/write endpoints.

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::http::header::CONTENT_TYPE;
use dp_model::{Document, has_errors};
use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

use crate::auth::{self, AuthUser};
use crate::authz::{self, Capability};
use crate::error::{ApiError, AppResult};
use crate::models::VersionSummary;
use crate::routes::files::{fetch_file, fetch_version_document};
use crate::services::audit;
use crate::state::AppState;

/// Document write response.
#[derive(Debug, Serialize)]
pub struct DocumentWriteResponse {
    /// Version created for the write.
    pub version: VersionSummary,
    /// Document revision after the write.
    pub revision: u64,
    /// Number of nodes in the document.
    pub node_count: usize,
}

/// Read the head document.
pub async fn get_document(
    State(state): State<AppState>,
    user: AuthUser,
    Path(file_id): Path<Uuid>,
) -> AppResult<Json<Value>> {
    authz::require_file(&state.db, user.user_id, file_id, Capability::View).await?;
    let file = fetch_file(&state, file_id).await?;
    let head = file
        .head_version_id
        .ok_or_else(|| ApiError::not_found("this file has no document yet"))?;
    let (document, _version) = fetch_version_document(&state, head).await?;
    Ok(Json(document))
}

/// Replace the head document with a validated document.
pub async fn put_document(
    State(state): State<AppState>,
    user: AuthUser,
    headers: HeaderMap,
    Path(file_id): Path<Uuid>,
    body: Bytes,
) -> AppResult<Json<DocumentWriteResponse>> {
    authz::require_file(&state.db, user.user_id, file_id, Capability::Edit).await?;
    require_json_content_type(&headers)?;
    let document = parse_document_body(&body)?;
    let canonical = document
        .to_json_value()
        .map_err(|err| ApiError::unprocessable("invalid_document", err.to_string()))?;

    let file = fetch_file(&state, file_id).await?;
    let mut tx = state.db.begin().await?;
    let version: VersionSummary = sqlx::query_as(
        "INSERT INTO file_versions (file_id, label, kind, parent_version_id, snapshot_json, created_by) \
         VALUES ($1, '', 'snapshot', $2, $3, $4) \
         RETURNING id, file_id, label, kind, parent_version_id, created_by, created_at, op_seq",
    )
    .bind(file_id)
    .bind(file.head_version_id)
    .bind(&canonical)
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
        "doc.update",
        "file",
        Some(&file_id.to_string()),
        serde_json::json!({
            "version_id": version.id,
            "revision": document.revision,
            "node_count": document.node_count(),
        }),
        Some(&auth::client_ip(&headers, None)),
    )
    .await?;
    tx.commit().await?;

    Ok(Json(DocumentWriteResponse {
        version,
        revision: document.revision,
        node_count: document.node_count(),
    }))
}

fn require_json_content_type(headers: &HeaderMap) -> AppResult<()> {
    let content_type = headers
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let is_json = content_type
        .split(';')
        .next()
        .map(str::trim)
        .is_some_and(|mime| mime == "application/json" || mime.ends_with("+json"));
    if is_json {
        Ok(())
    } else {
        Err(ApiError::unprocessable(
            "unsupported_media_type",
            "the request body must be sent as application/json",
        ))
    }
}

fn parse_document_body(body: &[u8]) -> AppResult<Document> {
    let value: Value = serde_json::from_slice(body).map_err(|err| {
        ApiError::bad_request_with(
            "invalid_json",
            format!("the request body is not valid JSON: {err}"),
        )
    })?;
    let document = Document::from_json_value(value).map_err(|err| {
        ApiError::unprocessable(
            "invalid_document",
            format!("the document failed model parsing: {err}"),
        )
    })?;
    let issues = document.validate();
    if has_errors(&issues) {
        return Err(ApiError::validation_failed(serde_json::json!({
            "issues": issues,
        })));
    }
    Ok(document)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;
    use serde_json::json;

    #[test]
    fn invalid_json_is_bad_request() {
        let error = parse_document_body(b"{not json").expect_err("invalid json");
        assert_eq!(error.status, StatusCode::BAD_REQUEST);
    }

    #[test]
    fn missing_schema_is_unprocessable() {
        let error = parse_document_body(br#"{"name":"x"}"#).expect_err("invalid document");
        assert_eq!(error.status, StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[test]
    fn valid_document_round_trips() {
        let document = Document::new("Server test");
        let value = document.to_json_value().expect("json");
        let bytes = serde_json::to_vec(&value).expect("bytes");
        let parsed = parse_document_body(&bytes).expect("parse");
        assert_eq!(parsed.name, "Server test");
        assert_eq!(parsed.node_count(), document.node_count());
    }

    #[test]
    fn unsupported_schema_is_unprocessable() {
        let mut value = Document::new("Future").to_json_value().expect("json");
        value["schema"] = json!(999_u32);
        let bytes = serde_json::to_vec(&value).expect("bytes");
        let error = parse_document_body(&bytes).expect_err("unsupported schema");
        assert_eq!(error.status, StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[test]
    fn content_type_must_be_json() {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, "text/plain".parse().expect("header"));
        assert!(require_json_content_type(&headers).is_err());
        headers.insert(
            CONTENT_TYPE,
            "application/json; charset=utf-8".parse().expect("header"),
        );
        assert!(require_json_content_type(&headers).is_ok());
        headers.insert(
            CONTENT_TYPE,
            "application/vnd.iconcrafter+json".parse().expect("header"),
        );
        assert!(require_json_content_type(&headers).is_ok());
    }
}
