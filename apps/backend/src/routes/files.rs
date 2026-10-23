//! File endpoints, collaborators and shared file helpers.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, Utc};
use dp_model::Document;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::auth::{self, AuthUser};
use crate::authz::{self, Capability, FileAccess, Role};
use crate::error::{ApiError, AppResult};
use crate::models::{FileView, Page, VersionSummary};
use crate::pagination::{PageParams, PageRequest};
use crate::services::audit;
use crate::state::AppState;
use crate::storage::validate_key;
use crate::validation;

/// File rename payload.
#[derive(Debug, Deserialize)]
pub struct UpdateFileRequest {
    /// New file name.
    pub name: String,
}

/// File detail response.
#[derive(Debug, Serialize)]
pub struct FileResponse {
    /// File metadata.
    pub file: FileView,
    /// Caller permissions.
    pub permissions: PermissionsPayload,
}

/// Effective permissions for a file.
#[derive(Debug, Clone, Serialize)]
pub struct PermissionsPayload {
    /// Role name.
    pub role: String,
    /// Capability names.
    pub capabilities: Vec<String>,
}

/// Build the permissions payload for a role.
pub fn permissions_payload(role: Role) -> PermissionsPayload {
    PermissionsPayload {
        role: role.as_str().to_owned(),
        capabilities: role
            .capability()
            .capabilities_upto()
            .into_iter()
            .map(str::to_owned)
            .collect(),
    }
}

/// Row shape shared by file handlers.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct FileRow {
    /// File id.
    pub id: Uuid,
    /// Owning project.
    pub project_id: Uuid,
    /// File name.
    pub name: String,
    /// Branch parent.
    pub branch_of: Option<Uuid>,
    /// Creator.
    pub created_by: Option<Uuid>,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Last update.
    pub updated_at: DateTime<Utc>,
    /// Head version.
    pub head_version_id: Option<Uuid>,
}

impl FileRow {
    /// Convert to the public representation.
    pub fn into_view(self, role: Role) -> FileView {
        FileView {
            id: self.id,
            project_id: self.project_id,
            name: self.name,
            branch_of: self.branch_of,
            created_by: self.created_by,
            created_at: self.created_at,
            updated_at: self.updated_at,
            head_version_id: self.head_version_id,
            role: role.as_str().to_owned(),
        }
    }
}

/// Fetch one non-deleted file row.
pub async fn fetch_file(state: &AppState, file_id: Uuid) -> AppResult<FileRow> {
    let row: Option<FileRow> = sqlx::query_as(
        "SELECT id, project_id, name, branch_of, created_by, created_at, updated_at, head_version_id \
         FROM files WHERE id = $1 AND is_deleted = false",
    )
    .bind(file_id)
    .fetch_optional(&state.db)
    .await?;
    row.ok_or_else(|| ApiError::not_found("file not found"))
}

/// Fetch the document JSON stored in a head/version snapshot.
pub async fn fetch_version_document(
    state: &AppState,
    version_id: Uuid,
) -> AppResult<(Value, VersionSummary)> {
    let version: Option<VersionSummary> = sqlx::query_as(
        "SELECT id, file_id, label, kind, parent_version_id, created_by, created_at, op_seq \
         FROM file_versions WHERE id = $1",
    )
    .bind(version_id)
    .fetch_optional(&state.db)
    .await?;
    let version = version.ok_or_else(|| ApiError::not_found("version not found"))?;
    let document: Option<Option<Value>> = sqlx::query_scalar(
        "SELECT COALESCE(snapshot_json, delta_json) FROM file_versions WHERE id = $1",
    )
    .bind(version_id)
    .fetch_optional(&state.db)
    .await?;
    let document = document
        .flatten()
        .ok_or_else(|| ApiError::not_found("version has no document payload"))?;
    Ok((document, version))
}

/// Create a file with its initial `branch_base` version in one transaction.
pub async fn create_file(
    state: &AppState,
    user_id: Uuid,
    project: &authz::ProjectAccess,
    name: &str,
    branch_of: Option<Uuid>,
    ip: &str,
) -> AppResult<FileView> {
    let name = validation::require_name(name, "name")?;
    let mut branch_parent_version: Option<Uuid> = None;
    let initial_document: Value = match branch_of {
        Some(parent_file_id) => {
            let parent = fetch_file(state, parent_file_id).await?;
            if parent.project_id != project.project_id {
                return Err(ApiError::bad_request_with(
                    "invalid_branch_parent",
                    "a branch must be created in the same project as its parent file",
                ));
            }
            let head = parent
                .head_version_id
                .ok_or_else(|| ApiError::conflict("the branch parent has no document yet"))?;
            let (document, _version) = fetch_version_document(state, head).await?;
            branch_parent_version = Some(head);
            document
        }
        None => Document::new(&name)
            .to_json_value()
            .map_err(|err| ApiError::internal_context("creating initial document", err))?,
    };

    let mut tx = state.db.begin().await?;
    let file: FileRow = sqlx::query_as(
        "INSERT INTO files (project_id, name, branch_of, created_by) VALUES ($1, $2, $3, $4) \
         RETURNING id, project_id, name, branch_of, created_by, created_at, updated_at, head_version_id",
    )
    .bind(project.project_id)
    .bind(&name)
    .bind(branch_of)
    .bind(user_id)
    .fetch_one(&mut *tx)
    .await?;

    let label = if branch_of.is_some() {
        "Branch base"
    } else {
        "Initial"
    };
    let version_id: Uuid = sqlx::query_scalar(
        "INSERT INTO file_versions (file_id, label, kind, parent_version_id, snapshot_json, created_by) \
         VALUES ($1, $2, 'branch_base', $3, $4, $5) RETURNING id",
    )
    .bind(file.id)
    .bind(label)
    .bind(branch_parent_version)
    .bind(&initial_document)
    .bind(user_id)
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query("UPDATE files SET head_version_id = $2 WHERE id = $1")
        .bind(file.id)
        .bind(version_id)
        .execute(&mut *tx)
        .await?;
    audit::record(
        &mut *tx,
        Some(user_id),
        "file.create",
        "file",
        Some(&file.id.to_string()),
        serde_json::json!({"name": name, "project_id": project.project_id, "branch_of": branch_of}),
        Some(ip),
    )
    .await?;
    tx.commit().await?;

    Ok(FileRow {
        head_version_id: Some(version_id),
        ..file
    }
    .into_view(authz::Role::Editor))
}

/// Fetch one file with permissions.
pub async fn get_file(
    State(state): State<AppState>,
    user: AuthUser,
    Path(file_id): Path<Uuid>,
) -> AppResult<Json<FileResponse>> {
    let access = authz::require_file(&state.db, user.user_id, file_id, Capability::View).await?;
    let row = fetch_file(&state, file_id).await?;
    Ok(Json(FileResponse {
        file: row.into_view(access.role),
        permissions: permissions_payload(access.role),
    }))
}

/// Rename a file.
pub async fn update_file(
    State(state): State<AppState>,
    user: AuthUser,
    headers: HeaderMap,
    Path(file_id): Path<Uuid>,
    Json(payload): Json<UpdateFileRequest>,
) -> AppResult<Json<FileResponse>> {
    let access = authz::require_file(&state.db, user.user_id, file_id, Capability::Edit).await?;
    let name = validation::require_name(&payload.name, "name")?;
    let row: Option<FileRow> = sqlx::query_as(
        "UPDATE files SET name = $2 WHERE id = $1 AND is_deleted = false \
         RETURNING id, project_id, name, branch_of, created_by, created_at, updated_at, head_version_id",
    )
    .bind(file_id)
    .bind(&name)
    .fetch_optional(&state.db)
    .await?;
    let row = row.ok_or_else(|| ApiError::not_found("file not found"))?;
    audit::record_pool(
        &state.db,
        Some(user.user_id),
        "file.update",
        "file",
        Some(&file_id.to_string()),
        serde_json::json!({"name": row.name}),
        Some(&auth::client_ip(&headers, None)),
    )
    .await?;
    Ok(Json(FileResponse {
        file: row.into_view(access.role),
        permissions: permissions_payload(access.role),
    }))
}

/// Soft-delete a file.
pub async fn delete_file(
    State(state): State<AppState>,
    user: AuthUser,
    headers: HeaderMap,
    Path(file_id): Path<Uuid>,
) -> AppResult<StatusCode> {
    authz::require_file(&state.db, user.user_id, file_id, Capability::Manage).await?;
    let mut tx = state.db.begin().await?;
    let updated =
        sqlx::query("UPDATE files SET is_deleted = true WHERE id = $1 AND is_deleted = false")
            .bind(file_id)
            .execute(&mut *tx)
            .await?;
    if updated.rows_affected() == 0 {
        return Err(ApiError::not_found("file not found"));
    }
    audit::record(
        &mut *tx,
        Some(user.user_id),
        "file.delete",
        "file",
        Some(&file_id.to_string()),
        serde_json::json!({}),
        Some(&auth::client_ip(&headers, None)),
    )
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Collaborator list entry.
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct CollaboratorView {
    /// User id.
    pub user_id: Uuid,
    /// E-mail.
    pub email: String,
    /// Display name.
    pub display_name: String,
    /// `view`, `comment` or `edit`.
    pub role: String,
    /// Grant timestamp.
    pub created_at: DateTime<Utc>,
}

/// Collaborator grant payload.
#[derive(Debug, Deserialize)]
pub struct AddCollaboratorRequest {
    /// E-mail of an existing account.
    pub email: String,
    /// `view`, `comment` or `edit`.
    pub role: String,
}

fn parse_collaborator_role(raw: &str) -> AppResult<&'static str> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "view" => Ok("view"),
        "comment" => Ok("comment"),
        "edit" => Ok("edit"),
        _ => Err(ApiError::validation_failed(serde_json::json!([
            {"field": "role", "message": "role must be one of view, comment, edit"}
        ]))),
    }
}

/// List explicit file collaborators.
pub async fn list_collaborators(
    State(state): State<AppState>,
    user: AuthUser,
    Path(file_id): Path<Uuid>,
) -> AppResult<Json<Vec<CollaboratorView>>> {
    authz::require_file(&state.db, user.user_id, file_id, Capability::View).await?;
    let rows: Vec<CollaboratorView> = sqlx::query_as(
        "SELECT c.user_id, u.email, u.display_name, c.role, c.created_at \
         FROM file_collaborators c JOIN users u ON u.id = c.user_id \
         WHERE c.file_id = $1 ORDER BY c.created_at ASC",
    )
    .bind(file_id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

/// Grant a collaborator role on a file.
pub async fn add_collaborator(
    State(state): State<AppState>,
    user: AuthUser,
    headers: HeaderMap,
    Path(file_id): Path<Uuid>,
    Json(payload): Json<AddCollaboratorRequest>,
) -> AppResult<Response> {
    authz::require_file(&state.db, user.user_id, file_id, Capability::Manage).await?;
    let role = parse_collaborator_role(&payload.role)?;
    let email = validation::require_email(&payload.email)?;
    let target: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM users WHERE lower(email) = $1 AND is_active = true")
            .bind(&email)
            .fetch_optional(&state.db)
            .await?;
    let target = target
        .ok_or_else(|| ApiError::not_found("no active account exists for that e-mail address"))?;
    sqlx::query(
        "INSERT INTO file_collaborators (file_id, user_id, role) VALUES ($1, $2, $3) \
         ON CONFLICT (file_id, user_id) DO UPDATE SET role = EXCLUDED.role",
    )
    .bind(file_id)
    .bind(target)
    .bind(role)
    .execute(&state.db)
    .await?;
    audit::record_pool(
        &state.db,
        Some(user.user_id),
        "file.collaborator_add",
        "file",
        Some(&file_id.to_string()),
        serde_json::json!({"user_id": target, "role": role}),
        Some(&auth::client_ip(&headers, None)),
    )
    .await?;
    let row: CollaboratorView = sqlx::query_as(
        "SELECT c.user_id, u.email, u.display_name, c.role, c.created_at \
         FROM file_collaborators c JOIN users u ON u.id = c.user_id \
         WHERE c.file_id = $1 AND c.user_id = $2",
    )
    .bind(file_id)
    .bind(target)
    .fetch_one(&state.db)
    .await?;
    Ok((StatusCode::CREATED, Json(row)).into_response())
}

/// Revoke a collaborator role.
pub async fn remove_collaborator(
    State(state): State<AppState>,
    user: AuthUser,
    headers: HeaderMap,
    Path((file_id, collaborator_id)): Path<(Uuid, Uuid)>,
) -> AppResult<StatusCode> {
    authz::require_file(&state.db, user.user_id, file_id, Capability::Manage).await?;
    let deleted = sqlx::query("DELETE FROM file_collaborators WHERE file_id = $1 AND user_id = $2")
        .bind(file_id)
        .bind(collaborator_id)
        .execute(&state.db)
        .await?;
    if deleted.rows_affected() == 0 {
        return Err(ApiError::not_found("file collaborator not found"));
    }
    audit::record_pool(
        &state.db,
        Some(user.user_id),
        "file.collaborator_remove",
        "file",
        Some(&file_id.to_string()),
        serde_json::json!({"user_id": collaborator_id}),
        Some(&auth::client_ip(&headers, None)),
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// List files in a project with the caller's effective role per file.
pub async fn list_project_files(
    State(state): State<AppState>,
    user: AuthUser,
    Path(project_id): Path<Uuid>,
    Query(params): Query<PageParams>,
) -> AppResult<Json<Page<FileView>>> {
    let access =
        authz::require_project(&state.db, user.user_id, project_id, Capability::View).await?;
    let page = PageRequest::resolve(params);
    let total: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM files WHERE project_id = $1 AND is_deleted = false",
    )
    .bind(project_id)
    .fetch_one(&state.db)
    .await?;
    let rows: Vec<FileListRow> = sqlx::query_as(
        "SELECT f.id, f.project_id, f.name, f.branch_of, f.created_by, f.created_at, f.updated_at, \
                f.head_version_id, c.role AS collaborator_role \
         FROM files f \
         LEFT JOIN file_collaborators c ON c.file_id = f.id AND c.user_id = $2 \
         WHERE f.project_id = $1 AND f.is_deleted = false \
         ORDER BY f.updated_at DESC, f.id ASC LIMIT $3 OFFSET $4",
    )
    .bind(project_id)
    .bind(user.user_id)
    .bind(i64::from(page.limit))
    .bind(page.offset)
    .fetch_all(&state.db)
    .await?;

    let items: Vec<FileView> = rows
        .into_iter()
        .map(|row| {
            let role = authz::file_role_from(
                access.role,
                user.user_id,
                row.created_by,
                row.collaborator_role.as_deref(),
            );
            row.into_file().into_view(role)
        })
        .collect();
    Ok(Json(Page::new(items, page, total)))
}

#[derive(Debug, sqlx::FromRow)]
struct FileListRow {
    id: Uuid,
    project_id: Uuid,
    name: String,
    branch_of: Option<Uuid>,
    created_by: Option<Uuid>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    head_version_id: Option<Uuid>,
    collaborator_role: Option<String>,
}

impl FileListRow {
    fn into_file(self) -> FileRow {
        FileRow {
            id: self.id,
            project_id: self.project_id,
            name: self.name,
            branch_of: self.branch_of,
            created_by: self.created_by,
            created_at: self.created_at,
            updated_at: self.updated_at,
            head_version_id: self.head_version_id,
        }
    }
}

/// Validate a storage key received from the database before streaming it.
pub fn ensure_storage_key(key: &str) -> AppResult<()> {
    validate_key(key).map_err(|err| ApiError::internal_context("invalid storage key", err))
}

/// Re-export for handlers that need the resolved access.
pub type ResolvedFileAccess = FileAccess;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collaborator_roles_parse_strictly() {
        assert_eq!(parse_collaborator_role("view").expect("view"), "view");
        assert_eq!(
            parse_collaborator_role("COMMENT").expect("comment"),
            "comment"
        );
        assert!(parse_collaborator_role("manage").is_err());
    }

    #[test]
    fn permissions_payload_orders_capabilities() {
        let payload = permissions_payload(Role::Commenter);
        assert_eq!(payload.role, "commenter");
        assert_eq!(payload.capabilities, vec!["view", "comment"]);
    }
}
