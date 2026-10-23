//! AppState shared by HTTP and WebSocket handlers.

use std::sync::Arc;

use sqlx::PgPool;

use crate::auth::rate_limit::RateLimiter;
use crate::config::Config;
use crate::storage::{ObjectStore, StoreError, build_store};
use crate::ws::oplog::{OpLog, PgOpLog};
use crate::ws::room::RoomRegistry;

/// Cheaply clonable application state.
#[derive(Clone)]
pub struct AppState {
    /// Static configuration.
    pub config: Arc<Config>,
    /// PostgreSQL pool.
    pub db: PgPool,
    /// Content-addressed object store.
    pub store: Arc<dyn ObjectStore>,
    /// In-process rate limiter.
    pub rate_limiter: Arc<RateLimiter>,
    /// Live WebSocket rooms.
    pub rooms: Arc<RoomRegistry>,
    /// Op journal used by the relay.
    pub op_log: Arc<dyn OpLog>,
    /// Process start time, used by health endpoints.
    pub started_at: chrono::DateTime<chrono::Utc>,
}

impl AppState {
    /// Build state from already constructed parts.
    pub fn new(config: Arc<Config>, db: PgPool, store: Arc<dyn ObjectStore>) -> Self {
        let rate_limiter = Arc::new(RateLimiter::new(config.rate_limit_per_minute));
        let op_log: Arc<dyn OpLog> = Arc::new(PgOpLog::new(db.clone()));
        Self {
            config,
            db,
            store,
            rate_limiter,
            rooms: Arc::new(RoomRegistry::new()),
            op_log,
            started_at: chrono::Utc::now(),
        }
    }

    /// Build state, constructing the configured object store.
    pub fn build(config: Arc<Config>, db: PgPool) -> Result<Self, StoreError> {
        let store = build_store(&config)?;
        Ok(Self::new(config, db, store))
    }
}

impl std::fmt::Debug for AppState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AppState")
            .field("config", &self.config)
            .field("store_kind", &self.store.kind())
            .finish_non_exhaustive()
    }
}
