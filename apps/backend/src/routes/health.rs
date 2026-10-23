//! Liveness and readiness endpoints.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

use crate::config::READY_DB_TIMEOUT;
use crate::error::ApiError;
use crate::state::AppState;

/// Liveness response.
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    /// Always `ok` when the process can answer.
    pub status: &'static str,
    /// Build version.
    pub version: &'static str,
    /// Seconds since the process started.
    pub uptime_seconds: u64,
}

/// Readiness response.
#[derive(Debug, Serialize)]
pub struct ReadyResponse {
    /// `ready` when dependencies answered.
    pub status: &'static str,
    /// Database probe result.
    pub database: &'static str,
    /// Object store kind.
    pub storage: &'static str,
}

/// Liveness probe: never touches dependencies.
pub async fn healthz(State(state): State<AppState>) -> Json<HealthResponse> {
    let uptime = (chrono::Utc::now() - state.started_at).num_seconds().max(0) as u64;
    Json(HealthResponse {
        status: "ok",
        version: crate::SERVER_VERSION,
        uptime_seconds: uptime,
    })
}

/// Readiness probe: checks the database with a bounded timeout.
pub async fn readyz(State(state): State<AppState>) -> Response {
    let probe = sqlx::query_scalar::<_, i32>("SELECT 1").fetch_one(&state.db);
    match tokio::time::timeout(READY_DB_TIMEOUT, probe).await {
        Ok(Ok(_)) => (
            StatusCode::OK,
            Json(ReadyResponse {
                status: "ready",
                database: "ok",
                storage: state.store.kind(),
            }),
        )
            .into_response(),
        Ok(Err(error)) => {
            tracing::warn!(error = %error, "readiness database check failed");
            readiness_failure("the database check failed")
        }
        Err(_) => {
            tracing::warn!("readiness database check timed out");
            readiness_failure("the database check timed out")
        }
    }
}

fn readiness_failure(message: &'static str) -> Response {
    let error = ApiError::unavailable("not_ready", message);
    error.into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn health_payload_shape() {
        let payload = HealthResponse {
            status: "ok",
            version: "0.1.0",
            uptime_seconds: 12,
        };
        let json = serde_json::to_value(&payload).expect("serialize");
        assert_eq!(json["status"], "ok");
        assert_eq!(json["uptime_seconds"], 12);
    }

    #[test]
    fn readiness_timeout_constant_is_bounded() {
        assert!(READY_DB_TIMEOUT >= Duration::from_millis(250));
        assert!(READY_DB_TIMEOUT <= Duration::from_secs(10));
    }
}
