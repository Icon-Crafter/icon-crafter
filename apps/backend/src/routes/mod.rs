//! HTTP routing.
//!
//! Every API response is JSON. Errors are always produced through
//! [`crate::error::ApiError`], which renders `{"error":{"code","message"}}`.
//! The router is assembled from three groups:
//!
//! * the authenticated, cookie or bearer authenticated `/api` group,
//! * the small set of public endpoints (published sites, share access,
//!   health checks and the WebSocket upgrade),
//! * an optional static file service for the built desktop front end.
//!
//! Middleware is applied after all routes have been merged so that every
//! route (including the fallbacks) is covered.

use axum::Router;
use axum::extract::{DefaultBodyLimit, State};
use axum::http::{HeaderName, HeaderValue, Request, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, patch, post};
use tower_http::compression::CompressionLayer;
use tower_http::cors::{AllowOrigin, Any, CorsLayer};
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;

use crate::auth::{csrf_middleware, method_not_allowed};
use crate::config::JSON_BODY_LIMIT_BYTES;
use crate::error::ApiError;
use crate::state::AppState;

use self::auth as auth_routes;

pub mod activity;
pub mod assets;
pub mod auth;
pub mod comments;
pub mod docs;
pub mod files;
pub mod health;
pub mod projects;
pub mod publish;
pub mod shares;
pub mod teams;
pub mod versions;

const REQUEST_ID_HEADER: &str = "x-request-id";
const CSRF_HEADER: &str = "x-csrf-token";

/// Builds the complete application router.
///
/// # Panics
///
/// Panics when two routes collide. Route tables are static, so this can only
/// be triggered by a programming error, never by user input.
pub fn router(state: AppState) -> Router {
    let config = state.config.clone();
    let upload_limit = config
        .max_upload_bytes
        .saturating_add(1024 * 1024)
        .max(JSON_BODY_LIMIT_BYTES + 1024 * 1024);
    let request_body_limit = upload_limit;

    let api = Router::new()
        // Authentication.
        .route("/api/auth/register", post(auth_routes::register))
        .route("/api/auth/login", post(auth_routes::login))
        .route("/api/auth/logout", post(auth_routes::logout))
        .route("/api/auth/me", get(auth_routes::me))
        // Teams and members.
        .route(
            "/api/teams",
            get(teams::list_teams).post(teams::create_team),
        )
        .route(
            "/api/teams/{team_id}",
            get(teams::get_team)
                .patch(teams::update_team)
                .delete(teams::delete_team),
        )
        .route(
            "/api/teams/{team_id}/members",
            get(teams::list_members).post(teams::add_member),
        )
        .route(
            "/api/teams/{team_id}/members/{user_id}",
            patch(teams::update_member).delete(teams::remove_member),
        )
        // Projects and files.
        .route(
            "/api/teams/{team_id}/projects",
            get(projects::list_projects).post(projects::create_project),
        )
        .route(
            "/api/projects/{project_id}",
            get(projects::get_project)
                .patch(projects::update_project)
                .delete(projects::delete_project),
        )
        .route(
            "/api/projects/{project_id}/files",
            get(files::list_project_files).post(projects::create_project_file),
        )
        .route(
            "/api/files/{file_id}",
            get(files::get_file)
                .patch(files::update_file)
                .delete(files::delete_file),
        )
        .route(
            "/api/files/{file_id}/doc",
            get(docs::get_document).put(docs::put_document),
        )
        // Versions.
        .route(
            "/api/files/{file_id}/versions",
            get(versions::list_versions).post(versions::create_named_version),
        )
        .route(
            "/api/files/{file_id}/versions/diff",
            get(versions::diff_versions),
        )
        .route(
            "/api/files/{file_id}/versions/{version_id}/restore",
            post(versions::restore_version),
        )
        // Comments.
        .route(
            "/api/files/{file_id}/comments",
            get(comments::list_comments).post(comments::create_comment),
        )
        .route(
            "/api/files/{file_id}/comments/{comment_id}",
            patch(comments::update_comment).delete(comments::delete_comment),
        )
        // Collaborators.
        .route(
            "/api/files/{file_id}/collaborators",
            get(files::list_collaborators).post(files::add_collaborator),
        )
        .route(
            "/api/files/{file_id}/collaborators/{user_id}",
            axum::routing::delete(files::remove_collaborator),
        )
        // Shares.
        .route(
            "/api/files/{file_id}/shares",
            get(shares::list_shares).post(shares::create_share),
        )
        .route(
            "/api/files/{file_id}/shares/{share_id}",
            axum::routing::delete(shares::revoke_share),
        )
        // Assets (multipart upload; streams up to the configured limit).
        .route(
            "/api/files/{file_id}/assets",
            get(assets::list_assets)
                .post(assets::upload_asset)
                .layer(DefaultBodyLimit::max(upload_limit)),
        )
        .route(
            "/api/files/{file_id}/assets/{asset_id}",
            get(assets::get_asset).delete(assets::delete_asset),
        )
        // Publishing and the audit log.
        .route("/api/files/{file_id}/publish", post(publish::publish_file))
        .route("/api/activity", get(activity::list_activity))
        // The default JSON request body limit; the multipart upload route
        // above overrides it with a larger, still bounded, limit.
        .layer(DefaultBodyLimit::max(JSON_BODY_LIMIT_BYTES))
        // CSRF defence applies to the whole authenticated API surface; the
        // middleware itself skips safe methods and bearer-token requests.
        .layer(middleware::from_fn_with_state(
            state.clone(),
            csrf_middleware,
        ));

    let public = Router::new()
        .route("/api/shares/{token}/access", post(shares::access_share))
        .route("/p/{slug}", get(publish::serve_published))
        .route("/healthz", get(health::healthz))
        .route("/readyz", get(health::readyz))
        .route("/ws/files/{file_id}", get(crate::ws::handler::ws_handler));

    let mut app = api.merge(public);

    if let Some(dir) = config.serve_static_dir.clone() {
        app = app.nest_service("/static", ServeDir::new(dir).precompressed_gzip());
    }

    app = app
        .fallback(not_found)
        .method_not_allowed_fallback(|| async { method_not_allowed() })
        .layer(RequestBodyLimitLayer::new(request_body_limit))
        .layer(CompressionLayer::new())
        .layer(PropagateRequestIdLayer::new(HeaderName::from_static(
            REQUEST_ID_HEADER,
        )))
        .layer(SetRequestIdLayer::new(
            HeaderName::from_static(REQUEST_ID_HEADER),
            MakeRequestUuid,
        ))
        .layer(TraceLayer::new_for_http())
        .layer(middleware::from_fn_with_state(
            state.clone(),
            normalize_errors,
        ));

    if let Some(cors) = cors_layer(&config.cors_origins) {
        app = app.layer(cors);
    }

    app.with_state(state)
}

/// JSON 404 for every unknown path.
async fn not_found() -> Response {
    ApiError::not_found("the requested resource does not exist").into_response()
}

/// Builds a strict CORS policy, or `None` when no origins are configured
/// (same-origin only). Configured values are validated by [`crate::config`],
/// so unparsable entries are skipped rather than treated as `*`.
fn cors_layer(origins: &[String]) -> Option<CorsLayer> {
    let values: Vec<HeaderValue> = origins
        .iter()
        .filter_map(|origin| HeaderValue::from_str(origin).ok())
        .collect();
    if values.is_empty() {
        return None;
    }
    Some(
        CorsLayer::new()
            .allow_origin(AllowOrigin::list(values))
            .allow_credentials(true)
            .allow_methods(Any)
            .allow_headers([
                header::CONTENT_TYPE,
                header::ACCEPT,
                header::AUTHORIZATION,
                HeaderName::from_static(CSRF_HEADER),
            ])
            .expose_headers([
                header::RETRY_AFTER,
                HeaderName::from_static(REQUEST_ID_HEADER),
            ]),
    )
}

/// Ensures errors produced by middleware and extractors that are not
/// [`ApiError`] (for example tower-http's plain-text `413`) are rendered with
/// the same JSON error envelope as every other response.
async fn normalize_errors(
    State(state): State<AppState>,
    request: Request<axum::body::Body>,
    next: Next,
) -> Response {
    let response = next.run(request).await;
    let status = response.status();
    if !status.is_client_error() && !status.is_server_error() {
        return response;
    }
    let is_json = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.starts_with("application/json"));
    if is_json {
        return response;
    }
    let error = match status {
        StatusCode::PAYLOAD_TOO_LARGE => ApiError::payload_too_large(
            "the request body exceeds the allowed size",
            state.config.max_upload_bytes as u64,
        ),
        StatusCode::NOT_FOUND => ApiError::not_found("the requested resource does not exist"),
        StatusCode::METHOD_NOT_ALLOWED => ApiError::new(
            StatusCode::METHOD_NOT_ALLOWED,
            "method_not_allowed",
            "the HTTP method is not supported for this endpoint",
        ),
        StatusCode::UNSUPPORTED_MEDIA_TYPE => ApiError::unprocessable(
            "unsupported_media_type",
            "this endpoint does not accept the request content type",
        ),
        StatusCode::BAD_REQUEST => {
            ApiError::bad_request("the request could not be processed as sent")
        }
        other => ApiError::new(
            other,
            "request_failed",
            "the request could not be completed",
        ),
    };
    error.into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cors_is_disabled_without_origins() {
        assert!(cors_layer(&[]).is_none());
    }

    #[test]
    fn cors_accepts_configured_origins() {
        let layer = cors_layer(&["https://app.example.com".to_string()]);
        assert!(layer.is_some());
    }

    #[test]
    fn request_id_header_is_lowercase() {
        assert_eq!(REQUEST_ID_HEADER, "x-request-id");
        assert_eq!(CSRF_HEADER, "x-csrf-token");
    }
}
