//! Authentication extractors, cookies and CSRF middleware.

pub mod csrf;
pub mod password;
pub mod rate_limit;
pub mod session;
pub mod tokens;

use std::collections::HashMap;
use std::net::SocketAddr;

use axum::extract::{ConnectInfo, FromRequestParts, Request, State};
use axum::http::{HeaderMap, Method, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use uuid::Uuid;

use crate::config::Config;
use crate::error::ApiError;
use crate::state::AppState;
use session::SessionUser;

/// Header used for bearer-token authentication (alternative to cookies).
pub const BEARER_PREFIX: &str = "Bearer ";

/// Authenticated user attached to protected handlers.
#[derive(Debug, Clone)]
pub struct AuthUser {
    /// Session id.
    pub session_id: Uuid,
    /// User id.
    pub user_id: Uuid,
    /// Normalized e-mail.
    pub email: String,
    /// Display name.
    pub display_name: String,
    /// Session expiry.
    pub expires_at: chrono::DateTime<chrono::Utc>,
    /// The raw session token, needed for logout.
    pub token: String,
}

/// Parse all `Cookie` headers into a simple map.
pub fn parse_cookies(headers: &HeaderMap) -> HashMap<String, String> {
    let mut cookies = HashMap::new();
    for value in headers.get_all(header::COOKIE) {
        let Ok(text) = value.to_str() else {
            continue;
        };
        for pair in text.split(';') {
            let pair = pair.trim();
            if pair.is_empty() {
                continue;
            }
            let (name, value) = match pair.split_once('=') {
                Some((name, value)) => (name.trim(), value.trim()),
                None => (pair, ""),
            };
            if !name.is_empty() {
                // Ignore malformed percent-encoding: tokens are plain hex.
                cookies.insert(name.to_owned(), value.to_owned());
            }
        }
    }
    cookies
}

/// Extract the session token from the `Authorization` header or cookie.
pub fn extract_session_token(headers: &HeaderMap, cookie_name: &str) -> Option<String> {
    if let Some(value) = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
    {
        let value = value.trim();
        if let Some(token) = value.strip_prefix(BEARER_PREFIX) {
            let token = token.trim();
            if !token.is_empty() {
                return Some(token.to_owned());
            }
        }
    }
    parse_cookies(headers).get(cookie_name).cloned()
}

/// Resolve a session from request headers. Used by the WebSocket upgrade and the
/// `me` handler; HTTP handlers normally use the [`AuthUser`] extractor.
pub async fn authenticate_headers(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<AuthUser, ApiError> {
    let token = extract_session_token(headers, &state.config.session_cookie_name)
        .ok_or_else(unauthorized)?;
    if !tokens::is_valid_token_shape(&token) {
        return Err(unauthorized());
    }
    let hash = tokens::token_hash(&token);
    let user = session::lookup_session(&state.db, &hash)
        .await?
        .ok_or_else(unauthorized)?;
    // Best effort: a failed touch must not break the request.
    let _ = session::touch_session(&state.db, user.session_id).await;
    Ok(AuthUser {
        session_id: user.session_id,
        user_id: user.user_id,
        email: user.email,
        display_name: user.display_name,
        expires_at: user.expires_at,
        token,
    })
}

fn unauthorized() -> ApiError {
    ApiError::unauthorized("authentication required")
}

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        authenticate_headers(state, &parts.headers).await
    }
}

/// Client IP used for rate limiting and audit records.
///
/// `X-Forwarded-For` is honoured because the server is expected to run behind a
/// reverse proxy (see README); only the first entry is used. Direct connections
/// fall back to the socket peer address.
pub fn client_ip(headers: &HeaderMap, connect: Option<SocketAddr>) -> String {
    let forwarded = headers
        .get("x-forwarded-for")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(',').next())
        .map(str::trim)
        .filter(|first| !first.is_empty() && first.parse::<std::net::IpAddr>().is_ok());
    if let Some(first) = forwarded {
        return first.to_owned();
    }
    if let Some(value) = headers.get("x-real-ip").and_then(|v| v.to_str().ok()) {
        let value = value.trim();
        if value.parse::<std::net::IpAddr>().is_ok() {
            return value.to_owned();
        }
    }
    connect
        .map(|address| address.ip().to_string())
        .unwrap_or_else(|| "unknown".to_owned())
}

/// Client IP from request parts (extensions carry `ConnectInfo`).
pub fn client_ip_from_parts(parts: &axum::http::request::Parts) -> String {
    let connect = parts
        .extensions
        .get::<ConnectInfo<SocketAddr>>()
        .map(|info| info.0);
    client_ip(&parts.headers, connect)
}

/// Enforce the configured auth rate limit for `scope`.
pub fn enforce_rate_limit(state: &AppState, ip: &str, scope: &str) -> Result<(), ApiError> {
    let key = format!("{ip}:{scope}");
    state
        .rate_limiter
        .check(&key)
        .map_err(ApiError::rate_limited)
}

fn cookie_attributes(config: &Config) -> String {
    let mut attributes = String::from("Path=/; SameSite=Lax");
    if config.cookie_secure {
        attributes.push_str("; Secure");
    }
    attributes
}

/// Build the `Set-Cookie` value for a session token.
pub fn session_cookie(config: &Config, token: &str) -> String {
    let max_age = config.session_ttl.as_secs();
    format!(
        "{}={}; HttpOnly; Max-Age={max_age}; {}",
        config.session_cookie_name,
        token,
        cookie_attributes(config)
    )
}

/// Build the `Set-Cookie` value for the CSRF double-submit token.
pub fn csrf_cookie(config: &Config, token: &str) -> String {
    let max_age = config.session_ttl.as_secs();
    format!(
        "{}={}; Max-Age={max_age}; {}",
        config.csrf_cookie_name,
        token,
        cookie_attributes(config)
    )
}

/// Build the `Set-Cookie` values that clear both auth cookies.
pub fn clear_auth_cookies(config: &Config) -> [String; 2] {
    let attributes = cookie_attributes(config);
    [
        format!(
            "{}=; HttpOnly; Max-Age=0; {attributes}",
            config.session_cookie_name
        ),
        format!("{}=; Max-Age=0; {attributes}", config.csrf_cookie_name),
    ]
}

/// Read the CSRF cookie value.
pub fn csrf_cookie_value(config: &Config, headers: &HeaderMap) -> Option<String> {
    parse_cookies(headers)
        .get(&config.csrf_cookie_name)
        .cloned()
}

/// CSRF middleware for cookie-authenticated state-changing requests.
pub async fn csrf_middleware(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    let method = request.method().clone();
    let headers = request.headers().clone();
    if !csrf::is_state_changing(&method) || csrf::uses_bearer_auth(&headers) {
        return Ok(next.run(request).await);
    }
    let cookies = parse_cookies(&headers);
    let Some(_session) = cookies.get(&state.config.session_cookie_name) else {
        // No ambient credential: the request will be rejected by auth or is a
        // public endpoint such as login/register/share access.
        return Ok(next.run(request).await);
    };
    let header_token = headers
        .get(csrf::CSRF_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::trim);
    let cookie_token = cookies
        .get(&state.config.csrf_cookie_name)
        .map(String::as_str);
    csrf::verify(header_token, cookie_token)?;
    Ok(next.run(request).await)
}

/// Build a JSON 405 response body for method-not-allowed fallbacks.
pub fn method_not_allowed() -> Response {
    (
        StatusCode::METHOD_NOT_ALLOWED,
        axum::Json(serde_json::json!({
            "error": {
                "code": "method_not_allowed",
                "message": "this method is not supported for the requested resource",
            }
        })),
    )
        .into_response()
}

/// Session expiry exposed to clients.
pub fn session_expiry(user: &SessionUser) -> chrono::DateTime<chrono::Utc> {
    user.expires_at
}

/// Normalize a user-agent header to a bounded, storable string.
pub fn user_agent(headers: &HeaderMap) -> Option<String> {
    headers
        .get(header::USER_AGENT)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.chars().take(400).collect())
}

/// Method helper used by the audit layer.
pub fn is_mutating(method: &Method) -> bool {
    csrf::is_state_changing(method)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (name, value) in pairs {
            map.append(
                axum::http::HeaderName::from_bytes(name.as_bytes()).expect("header name"),
                value.parse().expect("header value"),
            );
        }
        map
    }

    #[test]
    fn cookies_are_parsed() {
        let headers = headers(&[("cookie", "a=1; b=2; empty=")]);
        let cookies = parse_cookies(&headers);
        assert_eq!(cookies.get("a").map(String::as_str), Some("1"));
        assert_eq!(cookies.get("b").map(String::as_str), Some("2"));
        assert_eq!(cookies.get("empty").map(String::as_str), Some(""));
    }

    #[test]
    fn bearer_token_takes_precedence() {
        let headers = headers(&[
            ("authorization", "Bearer abcdef"),
            ("cookie", "ic_session=zzz"),
        ]);
        assert_eq!(
            extract_session_token(&headers, "ic_session").as_deref(),
            Some("abcdef")
        );
    }

    #[test]
    fn session_cookie_is_extracted() {
        let headers = headers(&[("cookie", "other=1; ic_session=abc")]);
        assert_eq!(
            extract_session_token(&headers, "ic_session").as_deref(),
            Some("abc")
        );
    }

    #[test]
    fn client_ip_prefers_forwarded_header() {
        let headers = headers(&[("x-forwarded-for", "203.0.113.7, 10.0.0.1")]);
        assert_eq!(
            client_ip(&headers, Some("127.0.0.1:1000".parse().expect("addr"))),
            "203.0.113.7"
        );
    }

    #[test]
    fn client_ip_falls_back_to_socket() {
        let headers = HeaderMap::new();
        assert_eq!(
            client_ip(&headers, Some("127.0.0.1:1000".parse().expect("addr"))),
            "127.0.0.1"
        );
        assert_eq!(client_ip(&headers, None), "unknown");
    }
}
