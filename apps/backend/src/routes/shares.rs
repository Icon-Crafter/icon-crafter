//! Share-link endpoints.

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::auth::password::{hash_password, verify_password};
use crate::auth::{self, AuthUser};
use crate::authz::{self, Capability, Role};
use crate::error::{ApiError, AppResult};
use crate::models::{Page, ShareView};
use crate::pagination::{PageParams, PageRequest};
use crate::routes::files::{fetch_file, fetch_version_document, permissions_payload};
use crate::services::audit;
use crate::state::AppState;

/// Create-share payload.
#[derive(Debug, Deserialize)]
pub struct CreateShareRequest {
    /// `view`, `comment` or `edit`.
    pub permission: String,
    /// Optional password protecting the link.
    #[serde(default)]
    pub password: Option<String>,
    /// Optional lifetime in hours.
    #[serde(default)]
    pub expires_in_hours: Option<u32>,
}

/// Created-share response (the raw token is only returned once).
#[derive(Debug, Serialize)]
pub struct CreatedShare {
    /// Share metadata.
    pub share: ShareView,
    /// Raw bearer token for the link.
    pub token: String,
    /// Absolute URL of the access endpoint.
    pub access_url: String,
}

/// Optional access payload.
#[derive(Debug, Default, Deserialize)]
pub struct AccessShareRequest {
    /// Password when the share is protected.
    #[serde(default)]
    pub password: Option<String>,
}

/// Public share access response.
#[derive(Debug, Serialize)]
pub struct ShareAccessResponse {
    /// Share metadata.
    pub share: ShareView,
    /// File summary.
    pub file: ShareFileInfo,
    /// Effective permissions for the share.
    pub permissions: crate::routes::files::PermissionsPayload,
    /// Current document, when the file has one.
    pub document: Option<Value>,
}

/// File summary embedded in the share-access response.
#[derive(Debug, Serialize)]
pub struct ShareFileInfo {
    /// File id.
    pub id: Uuid,
    /// File name.
    pub name: String,
    /// Owning project.
    pub project_id: Uuid,
}

#[derive(Debug, sqlx::FromRow)]
struct ShareRow {
    id: Uuid,
    file_id: Uuid,
    permission: String,
    has_password: bool,
    expires_at: Option<DateTime<Utc>>,
    created_by: Option<Uuid>,
    created_at: DateTime<Utc>,
    revoked_at: Option<DateTime<Utc>>,
    password_hash: Option<String>,
}

impl ShareRow {
    fn into_view(self) -> ShareView {
        ShareView {
            id: self.id,
            file_id: self.file_id,
            permission: self.permission,
            has_password: self.has_password,
            expires_at: self.expires_at,
            created_by: self.created_by,
            created_at: self.created_at,
            revoked_at: self.revoked_at,
        }
    }
}

/// Columns needed to render a share. Kept as a literal in each query so that
/// sqlx sees fully static SQL (no string building at all).
const LIST_SHARES_SQL: &str = "SELECT s.id, s.file_id, s.permission, \
        (s.password_hash IS NOT NULL) AS has_password, s.expires_at, s.created_by, \
        s.created_at, s.revoked_at, s.password_hash FROM shares s \
        WHERE s.file_id = $1 AND s.revoked_at IS NULL \
        ORDER BY s.created_at DESC LIMIT $2 OFFSET $3";

const FIND_SHARE_SQL: &str = "SELECT s.id, s.file_id, s.permission, \
        (s.password_hash IS NOT NULL) AS has_password, s.expires_at, s.created_by, \
        s.created_at, s.revoked_at, s.password_hash FROM shares s \
        JOIN files f ON f.id = s.file_id \
        JOIN projects p ON p.id = f.project_id \
        JOIN teams t ON t.id = p.team_id \
        WHERE s.token_hash = $1 AND f.is_deleted = false \
          AND p.is_deleted = false AND t.is_deleted = false";

/// List active share links for a file.
pub async fn list_shares(
    State(state): State<AppState>,
    user: AuthUser,
    Path(file_id): Path<Uuid>,
    Query(params): Query<PageParams>,
) -> AppResult<Json<Page<ShareView>>> {
    authz::require_file(&state.db, user.user_id, file_id, Capability::Edit).await?;
    let page = PageRequest::resolve(params);
    let total: i64 =
        sqlx::query_scalar("SELECT count(*) FROM shares WHERE file_id = $1 AND revoked_at IS NULL")
            .bind(file_id)
            .fetch_one(&state.db)
            .await?;
    let rows: Vec<ShareRow> = sqlx::query_as(LIST_SHARES_SQL)
        .bind(file_id)
        .bind(i64::from(page.limit))
        .bind(page.offset)
        .fetch_all(&state.db)
        .await?;
    let items = rows.into_iter().map(ShareRow::into_view).collect();
    Ok(Json(Page::new(items, page, total)))
}

/// Create a share link.
pub async fn create_share(
    State(state): State<AppState>,
    user: AuthUser,
    headers: HeaderMap,
    Path(file_id): Path<Uuid>,
    Json(payload): Json<CreateShareRequest>,
) -> AppResult<Response> {
    authz::require_file(&state.db, user.user_id, file_id, Capability::Edit).await?;
    let permission = parse_permission(&payload.permission)?;
    let expires_at = match payload.expires_in_hours {
        None => None,
        Some(hours) => {
            if !(1..=8760).contains(&hours) {
                return Err(ApiError::validation_failed(serde_json::json!([
                    {"field": "expires_in_hours", "message": "must be between 1 and 8760"}
                ])));
            }
            Some(Utc::now() + ChronoDuration::hours(i64::from(hours)))
        }
    };
    let password_hash = match payload.password.as_deref() {
        None => None,
        Some(raw) => {
            if raw.chars().count() < 8 {
                return Err(ApiError::validation_failed(serde_json::json!([
                    {"field": "password", "message": "must be at least 8 characters"}
                ])));
            }
            let raw = raw.to_owned();
            Some(
                tokio::task::spawn_blocking(move || hash_password(&raw))
                    .await
                    .map_err(|err| ApiError::internal_context("hashing share password", err))?
                    .map_err(|message| {
                        ApiError::internal(format!("hashing share password: {message}"))
                    })?,
            )
        }
    };

    let token = auth::tokens::generate_token()
        .map_err(|err| ApiError::internal(format!("generating share token: {err}")))?;
    let token_hash = auth::tokens::token_hash(&token);
    let mut tx = state.db.begin().await?;
    let share_id: Uuid = sqlx::query_scalar(
        "INSERT INTO shares (file_id, token_hash, permission, password_hash, expires_at, created_by) \
         VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
    )
    .bind(file_id)
    .bind(token_hash.as_slice())
    .bind(permission.as_str())
    .bind(&password_hash)
    .bind(expires_at)
    .bind(user.user_id)
    .fetch_one(&mut *tx)
    .await?;
    audit::record(
        &mut *tx,
        Some(user.user_id),
        "share.create",
        "share",
        Some(&share_id.to_string()),
        serde_json::json!({"file_id": file_id, "permission": permission.as_str(), "password": password_hash.is_some()}),
        Some(&auth::client_ip(&headers, None)),
    )
    .await?;
    tx.commit().await?;

    let insert_at: DateTime<Utc> = Utc::now();
    let share = ShareView {
        id: share_id,
        file_id,
        permission: permission.as_str().to_owned(),
        has_password: password_hash.is_some(),
        expires_at,
        created_by: Some(user.user_id),
        created_at: insert_at,
        revoked_at: None,
    };
    let access_url = format!("{}/api/shares/{token}/access", state.config.public_base_url);
    Ok((
        StatusCode::CREATED,
        Json(CreatedShare {
            share,
            token,
            access_url,
        }),
    )
        .into_response())
}

/// Revoke a share link.
pub async fn revoke_share(
    State(state): State<AppState>,
    user: AuthUser,
    headers: HeaderMap,
    Path((file_id, share_id)): Path<(Uuid, Uuid)>,
) -> AppResult<Response> {
    authz::require_file(&state.db, user.user_id, file_id, Capability::Edit).await?;
    let mut tx = state.db.begin().await?;
    let affected = sqlx::query(
        "UPDATE shares SET revoked_at = COALESCE(revoked_at, now()) \
         WHERE id = $1 AND file_id = $2 RETURNING id",
    )
    .bind(share_id)
    .bind(file_id)
    .fetch_optional(&mut *tx)
    .await?;
    if affected.is_none() {
        return Err(ApiError::not_found("share not found on this file"));
    }
    audit::record(
        &mut *tx,
        Some(user.user_id),
        "share.revoke",
        "share",
        Some(&share_id.to_string()),
        serde_json::json!({"file_id": file_id}),
        Some(&auth::client_ip(&headers, None)),
    )
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

/// Resolve a share token, enforcing expiry, revocation and optional password.
pub async fn access_share(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(token): Path<String>,
    body: Bytes,
) -> AppResult<Json<ShareAccessResponse>> {
    let ip = auth::client_ip(&headers, None);
    auth::enforce_rate_limit(&state, &ip, "share_access")?;
    if !auth::tokens::is_valid_token_shape(&token) {
        return Err(ApiError::not_found("share link not found"));
    }
    let payload: AccessShareRequest = if body.is_empty() {
        AccessShareRequest::default()
    } else {
        serde_json::from_slice(&body).map_err(|err| {
            ApiError::bad_request_with("invalid_json", format!("invalid JSON body: {err}"))
        })?
    };
    let token_hash = auth::tokens::token_hash(&token);
    let row: Option<ShareRow> = sqlx::query_as(FIND_SHARE_SQL)
        .bind(token_hash.as_slice())
        .fetch_optional(&state.db)
        .await?;
    let row = row.ok_or_else(|| ApiError::not_found("share link not found"))?;
    if row.revoked_at.is_some() {
        return Err(ApiError::forbidden_with(
            "share_revoked",
            "this share link has been revoked",
        ));
    }
    if let Some(expires_at) = row.expires_at
        && expires_at <= Utc::now()
    {
        return Err(ApiError::forbidden_with(
            "share_expired",
            "this share link has expired",
        ));
    }
    if let Some(password_hash) = row.password_hash.as_deref() {
        let password = payload
            .password
            .ok_or_else(|| ApiError::unauthorized("this share link requires a password"))?;
        let expected = password_hash.to_owned();
        let matched = tokio::task::spawn_blocking(move || verify_password(&password, &expected))
            .await
            .map_err(|err| ApiError::internal_context("verifying share password", err))?;
        if !matched {
            return Err(ApiError::forbidden_with(
                "share_password_invalid",
                "incorrect share password",
            ));
        }
    }

    let role = permission_role(&row.permission)
        .ok_or_else(|| ApiError::internal("share has an unknown permission value"))?;
    let permissions = {
        let mut permissions = permissions_payload(role);
        permissions.capabilities = share_capabilities(&row.permission);
        permissions
    };
    let file = fetch_file(&state, row.file_id).await?;
    let document = match file.head_version_id {
        Some(version_id) => {
            let (document, _version) = fetch_version_document(&state, version_id).await?;
            Some(document)
        }
        None => None,
    };
    let share = ShareRow {
        password_hash: None,
        ..row
    };
    Ok(Json(ShareAccessResponse {
        share: share.into_view(),
        file: ShareFileInfo {
            id: file.id,
            name: file.name,
            project_id: file.project_id,
        },
        permissions,
        document,
    }))
}

fn parse_permission(raw: &str) -> AppResult<Role> {
    match Role::from_collaborator_role(raw) {
        Some(role) => Ok(role),
        None => Err(ApiError::validation_failed(serde_json::json!([
            {"field": "permission", "message": "must be one of `view`, `comment` or `edit`"}
        ]))),
    }
}

fn permission_role(raw: &str) -> Option<Role> {
    Role::from_collaborator_role(raw)
}

/// Capabilities granted by a share permission (`view`, `comment`, `edit`).
pub(crate) fn share_capabilities(permission: &str) -> Vec<String> {
    let top = match permission {
        "view" => Capability::View,
        "comment" => Capability::Comment,
        "edit" => Capability::Edit,
        _ => return Vec::new(),
    };
    Capability::capabilities_upto(top)
        .into_iter()
        .map(str::to_owned)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permissions_round_trip() {
        assert_eq!(parse_permission("view").expect("view"), Role::Viewer);
        assert!(parse_permission("owner").is_err());
        assert!(parse_permission("").is_err());
    }

    #[test]
    fn share_capabilities_are_monotonic() {
        assert_eq!(share_capabilities("view"), vec!["view"]);
        assert_eq!(share_capabilities("comment"), vec!["view", "comment"]);
        assert_eq!(share_capabilities("edit"), vec!["view", "comment", "edit"]);
        assert!(share_capabilities("nope").is_empty());
    }

    #[test]
    fn share_view_hides_the_token() {
        let view = ShareView {
            id: Uuid::nil(),
            file_id: Uuid::nil(),
            permission: "view".to_owned(),
            has_password: false,
            expires_at: None,
            created_by: None,
            created_at: Utc::now(),
            revoked_at: None,
        };
        let json = serde_json::to_string(&view).expect("serialize");
        assert!(!json.contains("token"));
    }
}
