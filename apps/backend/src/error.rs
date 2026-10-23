//! One error type for every HTTP failure.
//!
//! Every handler returns `Result<_, ApiError>`; the `IntoResponse` implementation
//! renders the consistent JSON envelope
//!
//! ```json
//! { "error": { "code": "invalid_request", "message": "…", "details": … } }
//! ```
//!
//! Internal failures never leak implementation details to the client: the full
//! error is logged server-side and a generic message is returned.

use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use serde_json::json;

/// HTTP-facing error.
#[derive(Debug, Clone)]
pub struct ApiError {
    /// HTTP status code.
    pub status: StatusCode,
    /// Stable machine readable error code.
    pub code: String,
    /// Safe, human readable message.
    pub message: String,
    /// Optional structured details (validation issues, limits, …).
    pub details: Option<serde_json::Value>,
    /// Seconds to wait for a retryable request (429/503).
    pub retry_after_secs: Option<u64>,
}

impl ApiError {
    /// Build an error with an explicit status and code.
    pub fn new(
        status: StatusCode,
        code: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            status,
            code: code.into(),
            message: message.into(),
            details: None,
            retry_after_secs: None,
        }
    }

    /// Attach a structured `details` payload.
    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = Some(details);
        self
    }

    /// Set the `Retry-After` header value.
    pub fn with_retry_after(mut self, seconds: u64) -> Self {
        self.retry_after_secs = Some(seconds);
        self
    }

    /// 400 Bad Request.
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "invalid_request", message)
    }

    /// 400 Bad Request with a specific code.
    pub fn bad_request_with(code: &'static str, message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, code, message)
    }

    /// 401 Unauthorized.
    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self::new(StatusCode::UNAUTHORIZED, "unauthenticated", message)
    }

    /// 403 Forbidden.
    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, "forbidden", message)
    }

    /// 403 Forbidden with a specific code (for example `capability_required`).
    pub fn forbidden_with(code: &'static str, message: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, code, message)
    }

    /// 404 Not Found.
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, "not_found", message)
    }

    /// 404 Not Found with a specific code.
    pub fn not_found_with(code: &'static str, message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, code, message)
    }

    /// 409 Conflict.
    pub fn conflict(message: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, "conflict", message)
    }

    /// 409 Conflict with a specific code.
    pub fn conflict_with(code: &'static str, message: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, code, message)
    }

    /// 413 Payload Too Large.
    pub fn payload_too_large(message: impl Into<String>, limit_bytes: u64) -> Self {
        Self::new(StatusCode::PAYLOAD_TOO_LARGE, "payload_too_large", message)
            .with_details(json!({ "limit_bytes": limit_bytes }))
    }

    /// 422 Unprocessable Entity.
    pub fn unprocessable(code: &'static str, message: impl Into<String>) -> Self {
        Self::new(StatusCode::UNPROCESSABLE_ENTITY, code, message)
    }

    /// 422 with structured validation issues.
    pub fn validation_failed(issues: serde_json::Value) -> Self {
        Self::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "validation_failed",
            "the submitted data failed validation",
        )
        .with_details(issues)
    }

    /// 429 Too Many Requests.
    pub fn rate_limited(retry_after_secs: u64) -> Self {
        Self::new(
            StatusCode::TOO_MANY_REQUESTS,
            "rate_limited",
            "too many requests, slow down and retry later",
        )
        .with_retry_after(retry_after_secs)
    }

    /// 500 Internal Server Error. The supplied error is logged, never returned.
    pub fn internal(error: impl std::fmt::Display) -> Self {
        tracing::error!(error = %error, "internal server error");
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            "an internal error occurred",
        )
    }

    /// 500 with a context message that is logged together with the error.
    pub fn internal_context(context: &'static str, error: impl std::fmt::Display) -> Self {
        tracing::error!(error = %error, context, "internal server error");
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            "an internal error occurred",
        )
    }

    /// 503 Service Unavailable.
    pub fn unavailable(code: &'static str, message: impl Into<String>) -> Self {
        Self::new(StatusCode::SERVICE_UNAVAILABLE, code, message)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let mut error = json!({
            "code": self.code,
            "message": self.message,
        });
        if let Some(details) = self.details {
            if let Some(object) = error.as_object_mut() {
                object.insert("details".to_owned(), details);
            }
        }
        let body = json!({ "error": error });
        let mut response = (self.status, axum::Json(body)).into_response();
        if let Some(seconds) = self.retry_after_secs {
            if let Ok(value) = HeaderValue::from_str(&seconds.to_string()) {
                response.headers_mut().insert(header::RETRY_AFTER, value);
            }
        }
        response
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(error: sqlx::Error) -> Self {
        use sqlx::error::DatabaseError;
        match &error {
            sqlx::Error::RowNotFound => {
                ApiError::not_found("the requested resource does not exist")
            }
            sqlx::Error::PoolTimedOut => ApiError::unavailable(
                "database_unavailable",
                "the database is not accepting connections right now",
            ),
            sqlx::Error::PoolClosed => ApiError::unavailable(
                "database_unavailable",
                "the database is shutting down",
            ),
            sqlx::Error::Database(db) => match db.code().as_deref() {
                Some("23505") => ApiError::conflict("the resource already exists"),
                Some("23503") => ApiError::conflict(
                    "the operation conflicts with a referenced resource",
                ),
                Some("23514") | Some("22P02") | Some("22003") => ApiError::new(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "invalid_value",
                    "a submitted value violates a database constraint",
                ),
                Some("40001") | Some("40P01") => {
                    ApiError::internal_context("transaction serialization failure", error)
                }
                _ => {
                    tracing::error!(error = %error, code = db.code(), "database error");
                    ApiError::internal("database error")
                }
            },
            _ => {
                tracing::error!(error = %error, "database error");
                ApiError::internal("database error")
            }
        }
    }
}

/// Convenient result alias for handlers.
pub type AppResult<T> = Result<T, ApiError>;

/// Convert a "not found" `fetch_optional` result into an `ApiError`.
pub fn required<T>(value: Option<T>, message: &'static str) -> AppResult<T> {
    value.ok_or_else(|| ApiError::not_found(message))
}

/// Parse a path UUID, returning a 400 instead of panicking.
pub fn parse_uuid(value: &str, parameter: &'static str) -> AppResult<uuid::Uuid> {
    uuid::Uuid::parse_str(value).map_err(|_| {
        ApiError::bad_request_with(
            "invalid_parameter",
            format!("`{parameter}` must be a UUID"),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_envelope_shape() {
        let error = ApiError::bad_request("bad input");
        let response = error.into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let content_type = response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default();
        assert!(content_type.starts_with("application/json"));
    }

    #[test]
    fn retry_after_is_set() {
        let error = ApiError::rate_limited(17);
        let response = error.into_response();
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(
            response
                .headers()
                .get(header::RETRY_AFTER)
                .and_then(|v| v.to_str().ok()),
            Some("17")
        );
    }

    #[test]
    fn internal_errors_are_not_leaked() {
        let error = ApiError::internal("secret database password was wrong");
        assert_eq!(error.message, "an internal error occurred");
        assert_eq!(error.code, "internal_error");
    }

    #[test]
    fn invalid_uuid_is_a_bad_request() {
        let error = parse_uuid("not-a-uuid", "file_id").unwrap_err();
        assert_eq!(error.status, StatusCode::BAD_REQUEST);
        assert_eq!(error.code, "invalid_parameter");
    }

    #[test]
    fn no_rows_maps_to_not_found() {
        let error = ApiError::from(sqlx::Error::RowNotFound);
        assert_eq!(error.status, StatusCode::NOT_FOUND);
    }
}
