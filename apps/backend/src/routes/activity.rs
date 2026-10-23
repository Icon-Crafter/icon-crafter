//! Audit activity feed.

use axum::Json;
use axum::extract::{Query, State};
use serde::Deserialize;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::authz::{self, Capability};
use crate::error::AppResult;
use crate::models::{AuditEventView, Page};
use crate::pagination::{PageParams, PageRequest};
use crate::state::AppState;

/// Activity query.
#[derive(Debug, Default, Deserialize)]
pub struct ActivityQuery {
    /// Restrict to events about one file (requires view access).
    pub file_id: Option<Uuid>,
    /// Pagination.
    #[serde(flatten)]
    pub page: PageParams,
}

/// List audit events.
///
/// Without `file_id` only the caller's own actions are visible. With
/// `file_id` the caller must be able to view that file and sees every event
/// that targets it.
pub async fn list_activity(
    State(state): State<AppState>,
    user: AuthUser,
    Query(query): Query<ActivityQuery>,
) -> AppResult<Json<Page<AuditEventView>>> {
    let page = PageRequest::resolve(query.page);
    let (total, rows): (i64, Vec<AuditEventView>) = match query.file_id {
        Some(file_id) => {
            authz::require_file(&state.db, user.user_id, file_id, Capability::View).await?;
            let target = file_id.to_string();
            let total: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM audit_events e \
                 WHERE (e.target_type = 'file' AND e.target_id = $1) \
                    OR e.metadata ->> 'file_id' = $1",
            )
            .bind(&target)
            .fetch_one(&state.db)
            .await?;
            let rows: Vec<AuditEventView> = sqlx::query_as(
                "SELECT e.id, e.actor_id, e.action, e.target_type, e.target_id, e.metadata, e.ip, e.created_at \
                 FROM audit_events e \
                 WHERE (e.target_type = 'file' AND e.target_id = $1) \
                    OR e.metadata ->> 'file_id' = $1 \
                 ORDER BY e.id DESC LIMIT $2 OFFSET $3",
            )
            .bind(&target)
            .bind(i64::from(page.limit))
            .bind(page.offset)
            .fetch_all(&state.db)
            .await?;
            (total, rows)
        }
        None => {
            let total: i64 =
                sqlx::query_scalar("SELECT count(*) FROM audit_events WHERE actor_id = $1")
                    .bind(user.user_id)
                    .fetch_one(&state.db)
                    .await?;
            let rows: Vec<AuditEventView> = sqlx::query_as(
                "SELECT id, actor_id, action, target_type, target_id, metadata, ip, created_at \
                 FROM audit_events WHERE actor_id = $1 \
                 ORDER BY id DESC LIMIT $2 OFFSET $3",
            )
            .bind(user.user_id)
            .bind(i64::from(page.limit))
            .bind(page.offset)
            .fetch_all(&state.db)
            .await?;
            (total, rows)
        }
    };
    Ok(Json(Page::new(rows, page, total)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activity_query_defaults_to_own_events() {
        let query: ActivityQuery = serde_json::from_str("{}").expect("parse");
        assert!(query.file_id.is_none());
        assert!(query.page.page.is_none());
    }

    #[test]
    fn activity_query_parses_file_filter() {
        let id = Uuid::nil();
        let query: ActivityQuery =
            serde_json::from_value(serde_json::json!({"file_id": id})).expect("parse");
        assert_eq!(query.file_id, Some(id));
    }
}
