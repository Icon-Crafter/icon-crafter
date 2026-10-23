//! Session persistence: creation, lookup, touch and revocation.

use std::time::Duration;

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::{ApiError, AppResult};

/// Authenticated session joined with the owning user.
#[derive(Debug, Clone)]
pub struct SessionUser {
    /// Session row id.
    pub session_id: Uuid,
    /// Owning user id.
    pub user_id: Uuid,
    /// Normalized e-mail address.
    pub email: String,
    /// Display name.
    pub display_name: String,
    /// Session expiry.
    pub expires_at: DateTime<Utc>,
}

/// Create a session row and return its id.
pub async fn create_session(
    pool: &PgPool,
    user_id: Uuid,
    token_hash: &[u8],
    ttl: Duration,
    user_agent: Option<&str>,
    ip: Option<&str>,
) -> AppResult<Uuid> {
    let ttl = chrono::Duration::from_std(ttl)
        .map_err(|error| ApiError::internal_context("invalid session ttl", error))?;
    let expires_at = Utc::now() + ttl;
    let session_id: Uuid = sqlx::query_scalar(
        "INSERT INTO sessions (user_id, token_hash, expires_at, user_agent, ip) \
         VALUES ($1, $2, $3, $4, $5) RETURNING id",
    )
    .bind(user_id)
    .bind(token_hash)
    .bind(expires_at)
    .bind(user_agent)
    .bind(ip)
    .fetch_one(pool)
    .await?;
    Ok(session_id)
}

/// Look up a live session by token hash.
pub async fn lookup_session(pool: &PgPool, token_hash: &[u8]) -> AppResult<Option<SessionUser>> {
    let row: Option<(Uuid, Uuid, String, String, DateTime<Utc>)> = sqlx::query_as(
        "SELECT s.id, s.user_id, u.email, u.display_name, s.expires_at \
         FROM sessions s JOIN users u ON u.id = s.user_id \
         WHERE s.token_hash = $1 AND s.expires_at > now() AND u.is_active",
    )
    .bind(token_hash)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(
        |(session_id, user_id, email, display_name, expires_at)| SessionUser {
            session_id,
            user_id,
            email,
            display_name,
            expires_at,
        },
    ))
}

/// Refresh `last_seen_at`, at most once every five minutes per session.
pub async fn touch_session(pool: &PgPool, session_id: Uuid) -> AppResult<()> {
    sqlx::query(
        "UPDATE sessions SET last_seen_at = now() \
         WHERE id = $1 AND last_seen_at < now() - interval '5 minutes'",
    )
    .bind(session_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Delete a session by token hash. Returns whether a row was removed.
pub async fn delete_session(pool: &PgPool, token_hash: &[u8]) -> AppResult<bool> {
    let result = sqlx::query("DELETE FROM sessions WHERE token_hash = $1")
        .bind(token_hash)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

/// Delete every expired session (maintenance helper).
pub async fn delete_expired_sessions(pool: &PgPool) -> AppResult<u64> {
    let result = sqlx::query("DELETE FROM sessions WHERE expires_at <= now()")
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}
