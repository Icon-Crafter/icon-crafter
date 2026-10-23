//! Journal abstraction used by the WebSocket relay.
//!
//! The relay only needs an append-only, per-file ordered log. The production
//! implementation is PostgreSQL; tests use an in-memory log, and a future
//! `dp-collab` integration can implement the same trait for a different
//! replica backend without touching HTTP or WebSocket layers.

use std::future::Future;
use std::pin::Pin;

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::{ApiError, AppResult};
use crate::ws::protocol::OpsBatch;

/// Boxed future used by the dyn-compatible trait.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// One journaled op batch.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct JournalEntry {
    /// Per-file monotonic sequence.
    pub seq: i64,
    /// Client batch id.
    pub batch_id: Uuid,
    /// Acting user.
    pub actor_id: Option<Uuid>,
    /// Stored payload: `{ "base_seq": i64|null, "batch": OpsBatch }`.
    pub batch_json: Value,
    /// Insert timestamp.
    pub created_at: DateTime<Utc>,
}

/// Result of appending a batch.
#[derive(Debug, Clone, Copy)]
pub struct AppendOutcome {
    /// Sequence assigned to (or already owned by) the batch.
    pub seq: i64,
    /// Whether the batch was an idempotent replay.
    pub duplicate: bool,
    /// Journal timestamp.
    pub created_at: DateTime<Utc>,
}

/// Append-only per-file op log.
pub trait OpLog: Send + Sync {
    /// Append a batch, idempotent on `(file_id, batch_id)`.
    fn append(
        &self,
        file_id: Uuid,
        actor_id: Uuid,
        base_seq: Option<i64>,
        batch: &OpsBatch,
    ) -> BoxFuture<'_, AppResult<AppendOutcome>>;

    /// Highest journaled sequence for a file (`0` when empty).
    fn max_seq(&self, file_id: Uuid) -> BoxFuture<'_, AppResult<i64>>;

    /// Entries after `from_seq`, oldest first.
    fn since(
        &self,
        file_id: Uuid,
        from_seq: i64,
        limit: u32,
    ) -> BoxFuture<'_, AppResult<Vec<JournalEntry>>>;
}

/// PostgreSQL-backed journal.
#[derive(Debug, Clone)]
pub struct PgOpLog {
    pool: PgPool,
}

impl PgOpLog {
    /// Create a journal for a pool.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

impl OpLog for PgOpLog {
    fn append(
        &self,
        file_id: Uuid,
        actor_id: Uuid,
        base_seq: Option<i64>,
        batch: &OpsBatch,
    ) -> BoxFuture<'_, AppResult<AppendOutcome>> {
        let payload = match serde_json::to_value(serde_json::json!({
            "base_seq": base_seq,
            "batch": batch,
        })) {
            Ok(payload) => payload,
            Err(error) => {
                return Box::pin(async move {
                    Err(ApiError::internal_context("encoding op batch", error))
                });
            }
        };
        let batch_id = batch.batch_id;
        Box::pin(async move {
            let mut tx = self.pool.begin().await?;
            // Serialize winners per file so sequence allocation is dense and
            // deterministic even under concurrent writers.
            sqlx::query(
                "SELECT pg_advisory_xact_lock(hashtext('iconcrafter_op_journal'), hashtext($1))",
            )
            .bind(file_id.to_string())
            .execute(&mut *tx)
            .await?;

            let inserted: Option<(i64, DateTime<Utc>)> = sqlx::query_as(
                "INSERT INTO op_journal (file_id, batch_id, batch_json, actor_id, seq) \
                 SELECT $1, $2, $3, $4, COALESCE(MAX(seq), 0) + 1 \
                 FROM op_journal WHERE file_id = $1 \
                 ON CONFLICT (file_id, batch_id) DO NOTHING \
                 RETURNING seq, created_at",
            )
            .bind(file_id)
            .bind(batch_id)
            .bind(&payload)
            .bind(actor_id)
            .fetch_optional(&mut *tx)
            .await?;

            let outcome = match inserted {
                Some((seq, created_at)) => AppendOutcome {
                    seq,
                    duplicate: false,
                    created_at,
                },
                None => {
                    let existing: Option<(i64, DateTime<Utc>)> = sqlx::query_as(
                        "SELECT seq, created_at FROM op_journal \
                         WHERE file_id = $1 AND batch_id = $2",
                    )
                    .bind(file_id)
                    .bind(batch_id)
                    .fetch_optional(&mut *tx)
                    .await?;
                    match existing {
                        Some((seq, created_at)) => AppendOutcome {
                            seq,
                            duplicate: true,
                            created_at,
                        },
                        None => {
                            return Err(ApiError::internal(
                                "op batch vanished between insert and lookup",
                            ));
                        }
                    }
                }
            };
            tx.commit().await?;
            Ok(outcome)
        })
    }

    fn max_seq(&self, file_id: Uuid) -> BoxFuture<'_, AppResult<i64>> {
        Box::pin(async move {
            let max: Option<i64> =
                sqlx::query_scalar("SELECT MAX(seq) FROM op_journal WHERE file_id = $1")
                    .bind(file_id)
                    .fetch_one(&self.pool)
                    .await?;
            Ok(max.unwrap_or(0))
        })
    }

    fn since(
        &self,
        file_id: Uuid,
        from_seq: i64,
        limit: u32,
    ) -> BoxFuture<'_, AppResult<Vec<JournalEntry>>> {
        Box::pin(async move {
            let limit = i64::from(limit.clamp(1, 500));
            let entries: Vec<JournalEntry> = sqlx::query_as(
                "SELECT seq, batch_id, actor_id, batch_json, created_at \
                 FROM op_journal WHERE file_id = $1 AND seq > $2 \
                 ORDER BY seq ASC LIMIT $3",
            )
            .bind(file_id)
            .bind(from_seq.max(0))
            .bind(limit)
            .fetch_all(&self.pool)
            .await?;
            Ok(entries)
        })
    }
}

/// In-memory journal used by unit tests.
#[cfg(test)]
pub mod testing {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use chrono::Utc;

    use super::*;

    /// Deterministic in-memory `OpLog`.
    #[derive(Default)]
    pub struct MemoryOpLog {
        inner: Mutex<HashMap<Uuid, Vec<JournalEntry>>>,
    }

    impl MemoryOpLog {
        /// New empty log.
        pub fn new() -> Self {
            Self::default()
        }

        /// Number of journaled entries for a file.
        pub fn len(&self, file_id: Uuid) -> usize {
            self.inner
                .lock()
                .expect("memory op log poisoned")
                .get(&file_id)
                .map(Vec::len)
                .unwrap_or(0)
        }
    }

    impl OpLog for MemoryOpLog {
        fn append(
            &self,
            file_id: Uuid,
            actor_id: Uuid,
            base_seq: Option<i64>,
            batch: &OpsBatch,
        ) -> BoxFuture<'_, AppResult<AppendOutcome>> {
            let payload = serde_json::json!({"base_seq": base_seq, "batch": batch});
            let batch_id = batch.batch_id;
            Box::pin(async move {
                let mut inner = self.inner.lock().expect("memory op log poisoned");
                let entries = inner.entry(file_id).or_default();
                if let Some(existing) = entries.iter().find(|entry| entry.batch_id == batch_id) {
                    return Ok(AppendOutcome {
                        seq: existing.seq,
                        duplicate: true,
                        created_at: existing.created_at,
                    });
                }
                let seq = entries.iter().map(|entry| entry.seq).max().unwrap_or(0) + 1;
                let created_at = Utc::now();
                entries.push(JournalEntry {
                    seq,
                    batch_id,
                    actor_id: Some(actor_id),
                    batch_json: payload,
                    created_at,
                });
                Ok(AppendOutcome {
                    seq,
                    duplicate: false,
                    created_at,
                })
            })
        }

        fn max_seq(&self, file_id: Uuid) -> BoxFuture<'_, AppResult<i64>> {
            Box::pin(async move {
                let inner = self.inner.lock().expect("memory op log poisoned");
                Ok(inner
                    .get(&file_id)
                    .and_then(|entries| entries.iter().map(|entry| entry.seq).max())
                    .unwrap_or(0))
            })
        }

        fn since(
            &self,
            file_id: Uuid,
            from_seq: i64,
            limit: u32,
        ) -> BoxFuture<'_, AppResult<Vec<JournalEntry>>> {
            Box::pin(async move {
                let inner = self.inner.lock().expect("memory op log poisoned");
                let entries = inner.get(&file_id).cloned().unwrap_or_default();
                Ok(entries
                    .into_iter()
                    .filter(|entry| entry.seq > from_seq)
                    .take(limit as usize)
                    .collect())
            })
        }
    }
}
