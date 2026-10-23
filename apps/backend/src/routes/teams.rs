//! Team and membership endpoints.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::{self, AuthUser};
use crate::authz::{self, Capability, Role};
use crate::error::{ApiError, AppResult};
use crate::models::{Page, TeamMemberView, TeamSummary};
use crate::pagination::{PageParams, PageRequest};
use crate::services::audit;
use crate::state::AppState;
use crate::validation;

/// Team creation payload.
#[derive(Debug, Deserialize)]
pub struct CreateTeamRequest {
    /// Team name.
    pub name: String,
}

/// Team rename payload.
#[derive(Debug, Deserialize)]
pub struct UpdateTeamRequest {
    /// New team name.
    pub name: String,
}

/// Team detail.
#[derive(Debug, Serialize)]
pub struct TeamDetail {
    /// Team id.
    pub id: Uuid,
    /// Team name.
    pub name: String,
    /// Team creator.
    pub created_by: Option<Uuid>,
    /// Caller role.
    pub role: String,
    /// Number of members.
    pub member_count: i64,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Last update timestamp.
    pub updated_at: DateTime<Utc>,
}

/// Membership creation payload.
#[derive(Debug, Deserialize)]
pub struct AddMemberRequest {
    /// E-mail address of an existing account.
    pub email: String,
    /// `owner`, `admin`, `editor` or `viewer`.
    pub role: String,
}

/// Membership role update payload.
#[derive(Debug, Deserialize)]
pub struct UpdateMemberRequest {
    /// `owner`, `admin`, `editor` or `viewer`.
    pub role: String,
}

#[derive(Debug, sqlx::FromRow)]
struct TeamRow {
    id: Uuid,
    name: String,
    created_by: Option<Uuid>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

fn team_role_name(role: Role) -> &'static str {
    match role {
        Role::Owner => "owner",
        Role::Admin => "admin",
        Role::Editor => "editor",
        Role::Viewer => "viewer",
        Role::Commenter => "viewer",
    }
}

fn parse_team_role(raw: &str) -> AppResult<&'static str> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "owner" => Ok("owner"),
        "admin" => Ok("admin"),
        "editor" => Ok("editor"),
        "viewer" => Ok("viewer"),
        _ => Err(ApiError::validation_failed(serde_json::json!([
            {"field": "role", "message": "role must be one of owner, admin, editor, viewer"}
        ]))),
    }
}

async fn member_count(state: &AppState, team_id: Uuid) -> AppResult<i64> {
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM team_members WHERE team_id = $1")
        .bind(team_id)
        .fetch_one(&state.db)
        .await?;
    Ok(count)
}

async fn owner_count(state: &AppState, team_id: Uuid) -> AppResult<i64> {
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM team_members WHERE team_id = $1 AND role = 'owner'",
    )
    .bind(team_id)
    .fetch_one(&state.db)
    .await?;
    Ok(count)
}

/// List teams the caller belongs to.
pub async fn list_teams(
    State(state): State<AppState>,
    user: AuthUser,
    Query(params): Query<PageParams>,
) -> AppResult<Json<Page<TeamSummary>>> {
    let page = PageRequest::resolve(params);
    let total: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM teams t \
         WHERE t.is_deleted = false AND (t.created_by = $1 OR EXISTS (\
             SELECT 1 FROM team_members m WHERE m.team_id = t.id AND m.user_id = $1))",
    )
    .bind(user.user_id)
    .fetch_one(&state.db)
    .await?;

    let rows: Vec<(Uuid, String, String, i64, DateTime<Utc>)> = sqlx::query_as(
        "SELECT t.id, t.name, \
                COALESCE(m.role, 'owner') AS role, \
                (SELECT count(*) FROM team_members mm WHERE mm.team_id = t.id) AS member_count, \
                t.created_at \
         FROM teams t \
         LEFT JOIN team_members m ON m.team_id = t.id AND m.user_id = $1 \
         WHERE t.is_deleted = false AND (t.created_by = $1 OR m.user_id IS NOT NULL) \
         ORDER BY t.created_at DESC LIMIT $2 OFFSET $3",
    )
    .bind(user.user_id)
    .bind(i64::from(page.limit))
    .bind(page.offset)
    .fetch_all(&state.db)
    .await?;

    let items = rows
        .into_iter()
        .map(|(id, name, role, count, created_at)| TeamSummary {
            id,
            name,
            role,
            member_count: count,
            created_at,
        })
        .collect();
    Ok(Json(Page::new(items, page, total)))
}

/// Create a team; the caller becomes its owner.
pub async fn create_team(
    State(state): State<AppState>,
    user: AuthUser,
    headers: HeaderMap,
    Json(payload): Json<CreateTeamRequest>,
) -> AppResult<Response> {
    let name = validation::require_name(&payload.name, "name")?;
    let mut tx = state.db.begin().await?;
    let team: TeamRow = sqlx::query_as(
        "INSERT INTO teams (name, created_by) VALUES ($1, $2) \
         RETURNING id, name, created_by, created_at, updated_at",
    )
    .bind(&name)
    .bind(user.user_id)
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO team_members (team_id, user_id, role) VALUES ($1, $2, 'owner') \
         ON CONFLICT (team_id, user_id) DO UPDATE SET role = 'owner'",
    )
    .bind(team.id)
    .bind(user.user_id)
    .execute(&mut *tx)
    .await?;
    audit::record(
        &mut *tx,
        Some(user.user_id),
        "team.create",
        "team",
        Some(&team.id.to_string()),
        serde_json::json!({"name": team.name}),
        Some(&auth::client_ip(&headers, None)),
    )
    .await?;
    tx.commit().await?;

    let detail = TeamDetail {
        id: team.id,
        name: team.name,
        created_by: team.created_by,
        role: "owner".to_owned(),
        member_count: 1,
        created_at: team.created_at,
        updated_at: team.updated_at,
    };
    Ok((StatusCode::CREATED, Json(detail)).into_response())
}

/// Fetch one team.
pub async fn get_team(
    State(state): State<AppState>,
    user: AuthUser,
    Path(team_id): Path<Uuid>,
) -> AppResult<Json<TeamDetail>> {
    let access = authz::require_team(&state.db, user.user_id, team_id, Capability::View).await?;
    let team: TeamRow = sqlx::query_as(
        "SELECT id, name, created_by, created_at, updated_at FROM teams \
         WHERE id = $1 AND is_deleted = false",
    )
    .bind(team_id)
    .fetch_one(&state.db)
    .await?;
    Ok(Json(TeamDetail {
        id: team.id,
        name: team.name,
        created_by: team.created_by,
        role: team_role_name(access.role).to_owned(),
        member_count: member_count(&state, team_id).await?,
        created_at: team.created_at,
        updated_at: team.updated_at,
    }))
}

/// Rename a team.
pub async fn update_team(
    State(state): State<AppState>,
    user: AuthUser,
    headers: HeaderMap,
    Path(team_id): Path<Uuid>,
    Json(payload): Json<UpdateTeamRequest>,
) -> AppResult<Json<TeamDetail>> {
    authz::require_team(&state.db, user.user_id, team_id, Capability::Manage).await?;
    let name = validation::require_name(&payload.name, "name")?;
    let team: TeamRow = sqlx::query_as(
        "UPDATE teams SET name = $2 WHERE id = $1 AND is_deleted = false \
         RETURNING id, name, created_by, created_at, updated_at",
    )
    .bind(team_id)
    .bind(&name)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| ApiError::not_found("team not found"))?;
    audit::record_pool(
        &state.db,
        Some(user.user_id),
        "team.update",
        "team",
        Some(&team_id.to_string()),
        serde_json::json!({"name": team.name}),
        Some(&auth::client_ip(&headers, None)),
    )
    .await?;
    let access = authz::require_team(&state.db, user.user_id, team_id, Capability::View).await?;
    Ok(Json(TeamDetail {
        id: team.id,
        name: team.name,
        created_by: team.created_by,
        role: team_role_name(access.role).to_owned(),
        member_count: member_count(&state, team_id).await?,
        created_at: team.created_at,
        updated_at: team.updated_at,
    }))
}

/// Soft-delete a team. Only the team owner may do this.
pub async fn delete_team(
    State(state): State<AppState>,
    user: AuthUser,
    headers: HeaderMap,
    Path(team_id): Path<Uuid>,
) -> AppResult<StatusCode> {
    let access = authz::require_team(&state.db, user.user_id, team_id, Capability::Manage).await?;
    if access.role != Role::Owner {
        return Err(ApiError::forbidden("only the team owner can delete a team"));
    }
    let mut tx = state.db.begin().await?;
    let updated =
        sqlx::query("UPDATE teams SET is_deleted = true WHERE id = $1 AND is_deleted = false")
            .bind(team_id)
            .execute(&mut *tx)
            .await?;
    if updated.rows_affected() == 0 {
        return Err(ApiError::not_found("team not found"));
    }
    audit::record(
        &mut *tx,
        Some(user.user_id),
        "team.delete",
        "team",
        Some(&team_id.to_string()),
        serde_json::json!({}),
        Some(&auth::client_ip(&headers, None)),
    )
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

/// List team members.
pub async fn list_members(
    State(state): State<AppState>,
    user: AuthUser,
    Path(team_id): Path<Uuid>,
) -> AppResult<Json<Vec<TeamMemberView>>> {
    authz::require_team(&state.db, user.user_id, team_id, Capability::View).await?;
    let members: Vec<TeamMemberView> = sqlx::query_as(
        "SELECT m.user_id, u.email, u.display_name, m.role, m.created_at \
         FROM team_members m JOIN users u ON u.id = m.user_id \
         WHERE m.team_id = $1 ORDER BY m.created_at ASC",
    )
    .bind(team_id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(members))
}

/// Add an existing account to a team.
pub async fn add_member(
    State(state): State<AppState>,
    user: AuthUser,
    headers: HeaderMap,
    Path(team_id): Path<Uuid>,
    Json(payload): Json<AddMemberRequest>,
) -> AppResult<Response> {
    let access = authz::require_team(&state.db, user.user_id, team_id, Capability::Manage).await?;
    let role = parse_team_role(&payload.role)?;
    if role == "owner" && access.role != Role::Owner {
        return Err(ApiError::forbidden(
            "only the team owner can grant the owner role",
        ));
    }
    let email = validation::require_email(&payload.email)?;
    let target: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM users WHERE lower(email) = $1 AND is_active = true")
            .bind(&email)
            .fetch_optional(&state.db)
            .await?;
    let target = target
        .ok_or_else(|| ApiError::not_found("no active account exists for that e-mail address"))?;

    let mut tx = state.db.begin().await?;
    sqlx::query(
        "INSERT INTO team_members (team_id, user_id, role) VALUES ($1, $2, $3) \
         ON CONFLICT (team_id, user_id) DO UPDATE SET role = EXCLUDED.role",
    )
    .bind(team_id)
    .bind(target)
    .bind(role)
    .execute(&mut *tx)
    .await?;
    audit::record(
        &mut *tx,
        Some(user.user_id),
        "team.member_add",
        "team",
        Some(&team_id.to_string()),
        serde_json::json!({"user_id": target, "role": role}),
        Some(&auth::client_ip(&headers, None)),
    )
    .await?;
    tx.commit().await?;

    let member: TeamMemberView = sqlx::query_as(
        "SELECT m.user_id, u.email, u.display_name, m.role, m.created_at \
         FROM team_members m JOIN users u ON u.id = m.user_id \
         WHERE m.team_id = $1 AND m.user_id = $2",
    )
    .bind(team_id)
    .bind(target)
    .fetch_one(&state.db)
    .await?;
    Ok((StatusCode::CREATED, Json(member)).into_response())
}

/// Change one member's team role.
pub async fn update_member(
    State(state): State<AppState>,
    user: AuthUser,
    headers: HeaderMap,
    Path((team_id, member_user_id)): Path<(Uuid, Uuid)>,
    Json(payload): Json<UpdateMemberRequest>,
) -> AppResult<Json<TeamMemberView>> {
    let access = authz::require_team(&state.db, user.user_id, team_id, Capability::Manage).await?;
    let role = parse_team_role(&payload.role)?;
    let team_creator: Option<Uuid> =
        sqlx::query_scalar("SELECT created_by FROM teams WHERE id = $1 AND is_deleted = false")
            .bind(team_id)
            .fetch_optional(&state.db)
            .await?
            .flatten();
    if team_creator == Some(member_user_id) {
        return Err(ApiError::conflict(
            "the team owner's role cannot be changed",
        ));
    }
    if role == "owner" && access.role != Role::Owner {
        return Err(ApiError::forbidden(
            "only the team owner can grant the owner role",
        ));
    }
    let current: Option<String> =
        sqlx::query_scalar("SELECT role FROM team_members WHERE team_id = $1 AND user_id = $2")
            .bind(team_id)
            .bind(member_user_id)
            .fetch_optional(&state.db)
            .await?;
    let current = current.ok_or_else(|| ApiError::not_found("team member not found"))?;
    if current == "owner" && role != "owner" && owner_count(&state, team_id).await? <= 1 {
        return Err(ApiError::conflict("a team must keep at least one owner"));
    }

    let updated =
        sqlx::query("UPDATE team_members SET role = $3 WHERE team_id = $1 AND user_id = $2")
            .bind(team_id)
            .bind(member_user_id)
            .bind(role)
            .execute(&state.db)
            .await?;
    if updated.rows_affected() == 0 {
        return Err(ApiError::not_found("team member not found"));
    }
    audit::record_pool(
        &state.db,
        Some(user.user_id),
        "team.member_role",
        "team",
        Some(&team_id.to_string()),
        serde_json::json!({"user_id": member_user_id, "role": role}),
        Some(&auth::client_ip(&headers, None)),
    )
    .await?;
    let member: TeamMemberView = sqlx::query_as(
        "SELECT m.user_id, u.email, u.display_name, m.role, m.created_at \
         FROM team_members m JOIN users u ON u.id = m.user_id \
         WHERE m.team_id = $1 AND m.user_id = $2",
    )
    .bind(team_id)
    .bind(member_user_id)
    .fetch_one(&state.db)
    .await?;
    Ok(Json(member))
}

/// Remove a member from a team.
pub async fn remove_member(
    State(state): State<AppState>,
    user: AuthUser,
    headers: HeaderMap,
    Path((team_id, member_user_id)): Path<(Uuid, Uuid)>,
) -> AppResult<StatusCode> {
    authz::require_team(&state.db, user.user_id, team_id, Capability::Manage).await?;
    let team_creator: Option<Uuid> =
        sqlx::query_scalar("SELECT created_by FROM teams WHERE id = $1 AND is_deleted = false")
            .bind(team_id)
            .fetch_optional(&state.db)
            .await?
            .flatten();
    if team_creator == Some(member_user_id) {
        return Err(ApiError::conflict("the team owner cannot be removed"));
    }
    let current: Option<String> =
        sqlx::query_scalar("SELECT role FROM team_members WHERE team_id = $1 AND user_id = $2")
            .bind(team_id)
            .bind(member_user_id)
            .fetch_optional(&state.db)
            .await?;
    let Some(current) = current else {
        return Err(ApiError::not_found("team member not found"));
    };
    if current == "owner" && owner_count(&state, team_id).await? <= 1 {
        return Err(ApiError::conflict("a team must keep at least one owner"));
    }
    let mut tx = state.db.begin().await?;
    sqlx::query("DELETE FROM team_members WHERE team_id = $1 AND user_id = $2")
        .bind(team_id)
        .bind(member_user_id)
        .execute(&mut *tx)
        .await?;
    audit::record(
        &mut *tx,
        Some(user.user_id),
        "team.member_remove",
        "team",
        Some(&team_id.to_string()),
        serde_json::json!({"user_id": member_user_id}),
        Some(&auth::client_ip(&headers, None)),
    )
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn team_roles_parse_strictly() {
        assert_eq!(parse_team_role("owner").expect("owner"), "owner");
        assert_eq!(parse_team_role("ADMIN").expect("admin"), "admin");
        assert!(parse_team_role("superuser").is_err());
        assert!(parse_team_role("").is_err());
    }

    #[test]
    fn commenter_role_lists_as_viewer_for_teams() {
        assert_eq!(team_role_name(Role::Commenter), "viewer");
        assert_eq!(team_role_name(Role::Owner), "owner");
    }
}
