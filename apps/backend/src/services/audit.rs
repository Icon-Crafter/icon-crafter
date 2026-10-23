//! Audit trail service.

use serde_json::Value;
use sqlx::{PgPool, Postgres};
use uuid::Uuid;

use crate::error::AppResult;

/// Record one audit event.
///
/// Accepts either a pool or a transaction connection so callers can keep the
/// audit row in the same transaction as the state change it describes.
pub async fn record<'e, E>(
    executor: E,
    actor_id: Option<Uuid>,
    action: &str,
    target_type: &str,
    target_id: Option<&str>,
    metadata: Value,
    ip: Option<&str>,
) -> AppResult<()>
where
    E: sqlx::Executor<'e, Database = Postgres>,
{
    sqlx::query(
        "INSERT INTO audit_events (actor_id, action, target_type, target_id, metadata, ip) \
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(actor_id)
    .bind(action)
    .bind(target_type)
    .bind(target_id)
    .bind(metadata)
    .bind(ip)
    .execute(executor)
    .await?;
    Ok(())
}

/// Convenience wrapper that records an event on the pool.
pub async fn record_pool(
    pool: &PgPool,
    actor_id: Option<Uuid>,
    action: &str,
    target_type: &str,
    target_id: Option<&str>,
    metadata: Value,
    ip: Option<&str>,
) -> AppResult<()> {
    record(pool, actor_id, action, target_type, target_id, metadata, ip).await
}
