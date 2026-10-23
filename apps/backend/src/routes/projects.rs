//! Project endpoints.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::{self, AuthUser};
use crate::authz::{self, Capability};
use crate::error::{ApiError, AppResult};
use crate::models::{Page, ProjectView};
use crate::pagination::{PageParams, PageRequest};
use crate::routes::files::{self, permissions_payload};
use crate::services::audit;
use crate::state::AppState;
use crate::validation;

/// Project creation/rename payload.
#[derive(Debug, Deserialize)]
pub struct ProjectNameRequest {
    /// Project name.
    pub name: String,
}

/// New file payload.
#[derive(Debug, Deserialize)]
pub struct CreateFileRequest {
    /// File name.
    pub name: String,
    /// Optional parent file id when creating a branch.
    #[serde(default)]
    pub branch_of: Option<Uuid>,
}

/// Project detail with permissions.
#[derive(Debug, Serialize)]
pub struct ProjectResponse {
    /// Project metadata.
    pub project: ProjectView,
    /// Caller permissions.
    pub permissions: files::PermissionsPayload,
}

/// List projects of a team.
pub async fn list_projects(
    State(state): State<AppState>,
    user: AuthUser,
    Path(team_id): Path<Uuid>,
    Query(params): Query<PageParams>,
) -> AppResult<Json<Page<ProjectView>>> {
    authz::require_team(&state.db, user.user_id, team_id, Capability::View).await?;
    let page = PageRequest::resolve(params);
    let total: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM projects WHERE team_id = $1 AND is_deleted = false",
    )
    .bind(team_id)
    .fetch_one(&state.db)
    .await?;
    let rows: Vec<ProjectView> = sqlx::query_as(
        "SELECT id, team_id, name, created_at, updated_at FROM projects \
         WHERE team_id = $1 AND is_deleted = false \
         ORDER BY created_at DESC LIMIT $2 OFFSET $3",
    )
    .bind(team_id)
    .bind(i64::from(page.limit))
    .bind(page.offset)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(Page::new(rows, page, total)))
}

/// Create a project in a team.
pub async fn create_project(
    State(state): State<AppState>,
    user: AuthUser,
    headers: HeaderMap,
    Path(team_id): Path<Uuid>,
    Json(payload): Json<ProjectNameRequest>,
) -> AppResult<Response> {
    authz::require_team(&state.db, user.user_id, team_id, Capability::Edit).await?;
    let name = validation::require_name(&payload.name, "name")?;
    let project: ProjectView = sqlx::query_as(
        "INSERT INTO projects (team_id, name, created_by) VALUES ($1, $2, $3) \
         RETURNING id, team_id, name, created_at, updated_at",
    )
    .bind(team_id)
    .bind(&name)
    .bind(user.user_id)
    .fetch_one(&state.db)
    .await?;
    audit::record_pool(
        &state.db,
        Some(user.user_id),
        "project.create",
        "project",
        Some(&project.id.to_string()),
        serde_json::json!({"name": name, "team_id": team_id}),
        Some(&auth::client_ip(&headers, None)),
    )
    .await?;
    Ok((StatusCode::CREATED, Json(project)).into_response())
}

/// Fetch one project.
pub async fn get_project(
    State(state): State<AppState>,
    user: AuthUser,
    Path(project_id): Path<Uuid>,
) -> AppResult<Json<ProjectResponse>> {
    let access =
        authz::require_project(&state.db, user.user_id, project_id, Capability::View).await?;
    let project: ProjectView = sqlx::query_as(
        "SELECT id, team_id, name, created_at, updated_at FROM projects \
         WHERE id = $1 AND is_deleted = false",
    )
    .bind(project_id)
    .fetch_one(&state.db)
    .await?;
    Ok(Json(ProjectResponse {
        project,
        permissions: permissions_payload(access.role),
    }))
}

/// Rename a project.
pub async fn update_project(
    State(state): State<AppState>,
    user: AuthUser,
    headers: HeaderMap,
    Path(project_id): Path<Uuid>,
    Json(payload): Json<ProjectNameRequest>,
) -> AppResult<Json<ProjectResponse>> {
    let access =
        authz::require_project(&state.db, user.user_id, project_id, Capability::Edit).await?;
    let name = validation::require_name(&payload.name, "name")?;
    let project: Option<ProjectView> = sqlx::query_as(
        "UPDATE projects SET name = $2 WHERE id = $1 AND is_deleted = false \
         RETURNING id, team_id, name, created_at, updated_at",
    )
    .bind(project_id)
    .bind(&name)
    .fetch_optional(&state.db)
    .await?;
    let project = project.ok_or_else(|| ApiError::not_found("project not found"))?;
    audit::record_pool(
        &state.db,
        Some(user.user_id),
        "project.update",
        "project",
        Some(&project_id.to_string()),
        serde_json::json!({"name": name}),
        Some(&auth::client_ip(&headers, None)),
    )
    .await?;
    Ok(Json(ProjectResponse {
        project,
        permissions: permissions_payload(access.role),
    }))
}

/// Soft-delete a project.
pub async fn delete_project(
    State(state): State<AppState>,
    user: AuthUser,
    headers: HeaderMap,
    Path(project_id): Path<Uuid>,
) -> AppResult<StatusCode> {
    authz::require_project(&state.db, user.user_id, project_id, Capability::Manage).await?;
    let updated =
        sqlx::query("UPDATE projects SET is_deleted = true WHERE id = $1 AND is_deleted = false")
            .bind(project_id)
            .execute(&state.db)
            .await?;
    if updated.rows_affected() == 0 {
        return Err(ApiError::not_found("project not found"));
    }
    audit::record_pool(
        &state.db,
        Some(user.user_id),
        "project.delete",
        "project",
        Some(&project_id.to_string()),
        serde_json::json!({}),
        Some(&auth::client_ip(&headers, None)),
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Create a file in a project, optionally branching from another file.
pub async fn create_project_file(
    State(state): State<AppState>,
    user: AuthUser,
    headers: HeaderMap,
    Path(project_id): Path<Uuid>,
    Json(payload): Json<CreateFileRequest>,
) -> AppResult<Response> {
    let access =
        authz::require_project(&state.db, user.user_id, project_id, Capability::Edit).await?;
    let ip = auth::client_ip(&headers, None);
    let file = files::create_file(
        &state,
        user.user_id,
        &access,
        &payload.name,
        payload.branch_of,
        &ip,
    )
    .await?;
    Ok((StatusCode::CREATED, Json(file)).into_response())
}

/// Response shape for project metadata timestamps (used in tests).
#[derive(Debug, Clone, Serialize)]
pub struct ProjectTimestamps {
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Last update.
    pub updated_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_file_request_defaults_branch_to_none() {
        let request: CreateFileRequest = serde_json::from_str(r#"{"name":"Logo"}"#).expect("parse");
        assert_eq!(request.name, "Logo");
        assert!(request.branch_of.is_none());
    }

    #[test]
    fn create_file_request_parses_branch_parent() {
        let parent = Uuid::new_v4();
        let request: CreateFileRequest = serde_json::from_value(serde_json::json!({
            "name": "Logo v2",
            "branch_of": parent,
        }))
        .expect("parse");
        assert_eq!(request.branch_of, Some(parent));
    }
}
