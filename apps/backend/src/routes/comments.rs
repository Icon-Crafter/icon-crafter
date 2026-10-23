//! Comment endpoints: list, create, update (including resolve) and delete.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;

use crate::auth::{self, AuthUser};
use crate::authz::{self, Capability, Role};
use crate::error::{ApiError, AppResult};
use crate::models::{CommentView, Page};
use crate::pagination::{PageParams, PageRequest};
use crate::routes::files::{fetch_file, fetch_version_document};
use crate::services::audit;
use crate::state::AppState;
use crate::validation;

/// Comment list query.
#[derive(Debug, Default, Deserialize)]
pub struct CommentQuery {
    /// Filter to a single page.
    pub page_id: Option<Uuid>,
    /// Include resolved comments (`false` by default).
    pub include_resolved: Option<bool>,
    /// Pagination.
    #[serde(flatten)]
    pub page: PageParams,
}

/// Create-comment payload.
#[derive(Debug, Deserialize)]
pub struct CreateCommentRequest {
    /// Page the comment is anchored to.
    pub page_id: Uuid,
    /// Optional node anchor.
    #[serde(default)]
    pub node_id: Option<Uuid>,
    /// Comment body.
    pub body: String,
    /// Parent comment for replies.
    #[serde(default)]
    pub parent_id: Option<Uuid>,
    /// Canvas anchor X.
    #[serde(default)]
    pub anchor_x: Option<f64>,
    /// Canvas anchor Y.
    #[serde(default)]
    pub anchor_y: Option<f64>,
}

/// Update-comment payload.
#[derive(Debug, Default, Deserialize)]
pub struct UpdateCommentRequest {
    /// Replacement body.
    #[serde(default)]
    pub body: Option<String>,
    /// Set the resolution state (`true` resolves, `false` reopens).
    #[serde(default)]
    pub resolved: Option<bool>,
}

/// List comments on a file.
pub async fn list_comments(
    State(state): State<AppState>,
    user: AuthUser,
    Path(file_id): Path<Uuid>,
    Query(query): Query<CommentQuery>,
) -> AppResult<Json<Page<CommentView>>> {
    authz::require_file(&state.db, user.user_id, file_id, Capability::View).await?;
    let page = PageRequest::resolve(query.page);
    let include_resolved = query.include_resolved.unwrap_or(false);
    let total: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM comments \
         WHERE file_id = $1 \
           AND ($2::uuid IS NULL OR page_id = $2) \
           AND ($3 OR resolved_at IS NULL)",
    )
    .bind(file_id)
    .bind(query.page_id)
    .bind(include_resolved)
    .fetch_one(&state.db)
    .await?;
    let rows: Vec<CommentView> = sqlx::query_as(
        "SELECT c.id, c.file_id, c.page_id, c.node_id, c.author_id, \
                u.display_name AS author_name, c.body, c.parent_id, c.resolved_at, \
                c.resolved_by, c.anchor_x, c.anchor_y, c.created_at, c.updated_at \
         FROM comments c JOIN users u ON u.id = c.author_id \
         WHERE c.file_id = $1 \
           AND ($2::uuid IS NULL OR c.page_id = $2) \
           AND ($3 OR c.resolved_at IS NULL) \
         ORDER BY c.created_at ASC, c.id ASC LIMIT $4 OFFSET $5",
    )
    .bind(file_id)
    .bind(query.page_id)
    .bind(include_resolved)
    .bind(i64::from(page.limit))
    .bind(page.offset)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(Page::new(rows, page, total)))
}

/// Create a comment on a file.
pub async fn create_comment(
    State(state): State<AppState>,
    user: AuthUser,
    headers: HeaderMap,
    Path(file_id): Path<Uuid>,
    Json(payload): Json<CreateCommentRequest>,
) -> AppResult<Response> {
    authz::require_file(&state.db, user.user_id, file_id, Capability::Comment).await?;
    let body = validation::validate_comment_body(&payload.body).map_err(|message| {
        ApiError::validation_failed(serde_json::json!([{"field": "body", "message": message}]))
    })?;
    validate_anchor(payload.anchor_x, payload.anchor_y)?;
    ensure_page_exists(&state, file_id, payload.page_id).await?;
    if let Some(parent_id) = payload.parent_id {
        let exists: Option<Uuid> =
            sqlx::query_scalar("SELECT id FROM comments WHERE id = $1 AND file_id = $2")
                .bind(parent_id)
                .bind(file_id)
                .fetch_optional(&state.db)
                .await?;
        if exists.is_none() {
            return Err(ApiError::unprocessable(
                "invalid_parent",
                "the parent comment does not exist on this file",
            ));
        }
    }

    let mut tx = state.db.begin().await?;
    let comment: CommentView = sqlx::query_as(
        "WITH inserted AS ( \
            INSERT INTO comments \
                (file_id, page_id, node_id, author_id, body, parent_id, anchor_x, anchor_y) \
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8) \
            RETURNING * \
         ) \
         SELECT i.id, i.file_id, i.page_id, i.node_id, i.author_id, \
                u.display_name AS author_name, i.body, i.parent_id, i.resolved_at, \
                i.resolved_by, i.anchor_x, i.anchor_y, i.created_at, i.updated_at \
         FROM inserted i JOIN users u ON u.id = i.author_id",
    )
    .bind(file_id)
    .bind(payload.page_id)
    .bind(payload.node_id)
    .bind(user.user_id)
    .bind(&body)
    .bind(payload.parent_id)
    .bind(payload.anchor_x)
    .bind(payload.anchor_y)
    .fetch_one(&mut *tx)
    .await?;
    audit::record(
        &mut *tx,
        Some(user.user_id),
        "comment.create",
        "comment",
        Some(&comment.id.to_string()),
        serde_json::json!({"file_id": file_id, "page_id": payload.page_id}),
        Some(&auth::client_ip(&headers, None)),
    )
    .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(comment)).into_response())
}

/// Update or resolve a comment.
pub async fn update_comment(
    State(state): State<AppState>,
    user: AuthUser,
    headers: HeaderMap,
    Path((file_id, comment_id)): Path<(Uuid, Uuid)>,
    Json(payload): Json<UpdateCommentRequest>,
) -> AppResult<Json<CommentView>> {
    let access = authz::require_file(&state.db, user.user_id, file_id, Capability::Comment).await?;
    let author_id: Option<Uuid> =
        sqlx::query_scalar("SELECT author_id FROM comments WHERE id = $1 AND file_id = $2")
            .bind(comment_id)
            .bind(file_id)
            .fetch_optional(&state.db)
            .await?;
    let author_id =
        author_id.ok_or_else(|| ApiError::not_found("comment not found on this file"))?;
    let is_author = author_id == user.user_id;

    let body = match payload.body {
        Some(raw) => {
            if !can_modify_comment(access.role, is_author) {
                return Err(ApiError::forbidden(
                    "only the comment author can edit this comment",
                ));
            }
            Some(validation::validate_comment_body(&raw).map_err(|message| {
                ApiError::validation_failed(
                    serde_json::json!([{"field": "body", "message": message}]),
                )
            })?)
        }
        None => None,
    };
    if body.is_none() && payload.resolved.is_none() {
        return Err(ApiError::bad_request(
            "provide `body` or `resolved` to update a comment",
        ));
    }
    let resolving = payload.resolved;
    let mut tx = state.db.begin().await?;
    let comment: CommentView = sqlx::query_as(
        "WITH updated AS ( \
            UPDATE comments SET \
                body = COALESCE($3, body), \
                resolved_at = CASE WHEN $4::boolean IS NULL THEN resolved_at \
                                   WHEN $4 THEN COALESCE(resolved_at, now()) ELSE NULL END, \
                resolved_by = CASE WHEN $4::boolean IS NULL THEN resolved_by \
                                   WHEN $4 THEN $5 ELSE NULL END \
            WHERE id = $1 AND file_id = $2 \
            RETURNING * \
         ) \
         SELECT u0.id, u0.file_id, u0.page_id, u0.node_id, u0.author_id, \
                u1.display_name AS author_name, u0.body, u0.parent_id, u0.resolved_at, \
                u0.resolved_by, u0.anchor_x, u0.anchor_y, u0.created_at, u0.updated_at \
         FROM updated u0 JOIN users u1 ON u1.id = u0.author_id",
    )
    .bind(comment_id)
    .bind(file_id)
    .bind(body)
    .bind(resolving)
    .bind(user.user_id)
    .fetch_one(&mut *tx)
    .await?;
    audit::record(
        &mut *tx,
        Some(user.user_id),
        if resolving.is_some() {
            "comment.resolve"
        } else {
            "comment.update"
        },
        "comment",
        Some(&comment_id.to_string()),
        serde_json::json!({"file_id": file_id}),
        Some(&auth::client_ip(&headers, None)),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(comment))
}

/// Delete a comment (replies are removed by the foreign key cascade).
pub async fn delete_comment(
    State(state): State<AppState>,
    user: AuthUser,
    headers: HeaderMap,
    Path((file_id, comment_id)): Path<(Uuid, Uuid)>,
) -> AppResult<Response> {
    let access = authz::require_file(&state.db, user.user_id, file_id, Capability::Comment).await?;
    let author_id: Option<Uuid> =
        sqlx::query_scalar("SELECT author_id FROM comments WHERE id = $1 AND file_id = $2")
            .bind(comment_id)
            .bind(file_id)
            .fetch_optional(&state.db)
            .await?;
    let author_id =
        author_id.ok_or_else(|| ApiError::not_found("comment not found on this file"))?;
    let is_author = author_id == user.user_id;
    if !can_modify_comment(access.role, is_author) {
        return Err(ApiError::forbidden(
            "only the comment author can delete this comment",
        ));
    }

    let mut tx = state.db.begin().await?;
    sqlx::query("DELETE FROM comments WHERE id = $1 AND file_id = $2")
        .bind(comment_id)
        .bind(file_id)
        .execute(&mut *tx)
        .await?;
    audit::record(
        &mut *tx,
        Some(user.user_id),
        "comment.delete",
        "comment",
        Some(&comment_id.to_string()),
        serde_json::json!({"file_id": file_id}),
        Some(&auth::client_ip(&headers, None)),
    )
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

fn validate_anchor(anchor_x: Option<f64>, anchor_y: Option<f64>) -> AppResult<()> {
    for (name, value) in [("anchor_x", anchor_x), ("anchor_y", anchor_y)] {
        if let Some(number) = value
            && !number.is_finite()
        {
            return Err(ApiError::validation_failed(serde_json::json!([
                {"field": name, "message": "the anchor must be a finite number"}
            ])));
        }
    }
    Ok(())
}

async fn ensure_page_exists(state: &AppState, file_id: Uuid, page_id: Uuid) -> AppResult<()> {
    let file = fetch_file(state, file_id).await?;
    let head = file.head_version_id.ok_or_else(|| {
        ApiError::conflict("this file has no document yet, so comments cannot be anchored")
    })?;
    let (document, _version) = fetch_version_document(state, head).await?;
    let pages = document.get("pages").and_then(Value::as_array);
    let found = pages.is_some_and(|pages| {
        pages.iter().any(|page| {
            page.get("id")
                .and_then(Value::as_str)
                .is_some_and(|raw| raw == page_id.to_string())
        })
    });
    if found {
        Ok(())
    } else {
        Err(ApiError::unprocessable(
            "invalid_page",
            "the page does not exist in this file's current document",
        ))
    }
}

/// Helper used by tests to reason about author-edit permissions.
pub(crate) fn can_modify_comment(role: Role, is_author: bool) -> bool {
    is_author || role.management_capability() >= Capability::Manage
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn managers_and_authors_can_modify_comments() {
        assert!(can_modify_comment(Role::Viewer, true));
        assert!(!can_modify_comment(Role::Editor, false));
        assert!(can_modify_comment(Role::Owner, false));
    }

    #[test]
    fn non_finite_anchors_are_rejected() {
        assert!(validate_anchor(Some(f64::NAN), None).is_err());
        assert!(validate_anchor(Some(1.0), Some(f64::INFINITY)).is_err());
        assert!(validate_anchor(Some(1.0), Some(2.0)).is_ok());
        assert!(validate_anchor(None, None).is_ok());
    }

    #[test]
    fn comment_query_defaults() {
        let query: CommentQuery = serde_json::from_str("{}").expect("parse");
        assert_eq!(query.include_resolved, None);
        assert!(query.page_id.is_none());
    }
}
