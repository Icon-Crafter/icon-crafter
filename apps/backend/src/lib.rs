//! `iconcrafter-server` — the Icon Crafter collaboration and publishing
//! backend.
//!
//! The crate is organised as a library plus a thin binary so that handlers,
//! storage backends and the WebSocket relay can be exercised from tests
//! without starting a network listener.
//!
//! * [`config`] — environment driven configuration and validation.
//! * [`error`] — the single JSON error envelope used by every route.
//! * [`auth`] — password hashing, session tokens, CSRF and rate limiting.
//! * [`authz`] — team, project and file capability resolution.
//! * [`routes`] — the Axum router and the HTTP endpoint modules.
//! * [`storage`] — the [`storage::ObjectStore`] trait with local and S3
//!   implementations, plus upload magic-byte sniffing.
//! * [`ws`] — the per-file collaboration relay, room registry and op log.

pub mod auth;
pub mod authz;
pub mod config;
pub mod error;
pub mod models;
pub mod pagination;
pub mod routes;
pub mod services;
pub mod state;
pub mod storage;
pub mod validation;
pub mod ws;

pub use config::Config;
pub use error::{ApiError, AppResult};
pub use routes::router;
pub use state::AppState;

/// Version of the running server, exposed by `GET /healthz`.
pub const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");
