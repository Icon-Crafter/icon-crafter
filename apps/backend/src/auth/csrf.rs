//! CSRF protection for cookie-authenticated, state-changing requests.
//!
//! The server uses the double-submit pattern: a random token is stored in a
//! non-`HttpOnly` cookie and must be echoed in the `X-CSRF-Token` header. A
//! cross-site attacker can make the browser send the cookie but cannot read it,
//! so the header will be missing or wrong. Requests authenticated with an
//! `Authorization: Bearer` header are exempt because they are not ambient
//! credentials.

use axum::http::{HeaderMap, Method};
use subtle::ConstantTimeEq;

use crate::error::ApiError;

/// Header carrying the CSRF token.
pub const CSRF_HEADER: &str = "x-csrf-token";

/// Whether a method needs CSRF protection.
pub fn is_state_changing(method: &Method) -> bool {
    !matches!(
        *method,
        Method::GET | Method::HEAD | Method::OPTIONS | Method::TRACE
    )
}

/// Verify that the header token matches the cookie token in constant time.
pub fn verify(header_token: Option<&str>, cookie_token: Option<&str>) -> Result<(), ApiError> {
    let (Some(header_token), Some(cookie_token)) = (header_token, cookie_token) else {
        return Err(csrf_error());
    };
    if header_token.len() != cookie_token.len()
        || !bool::from(header_token.as_bytes().ct_eq(cookie_token.as_bytes()))
    {
        return Err(csrf_error());
    }
    Ok(())
}

/// Whether the request authenticates with a bearer token (CSRF not applicable).
pub fn uses_bearer_auth(headers: &HeaderMap) -> bool {
    headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.trim_start().starts_with("Bearer "))
}

fn csrf_error() -> ApiError {
    ApiError::forbidden_with(
        "csrf_check_failed",
        "missing or invalid CSRF token; send the value of the CSRF cookie in the `X-CSRF-Token` header",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matching_tokens_pass() {
        assert!(verify(Some("abc123"), Some("abc123")).is_ok());
    }

    #[test]
    fn mismatches_and_missing_values_fail() {
        assert!(verify(Some("abc123"), Some("abc124")).is_err());
        assert!(verify(Some("abc"), Some("abc123")).is_err());
        assert!(verify(None, Some("abc123")).is_err());
        assert!(verify(Some("abc123"), None).is_err());
        assert!(verify(None, None).is_err());
    }

    #[test]
    fn safe_methods_do_not_need_csrf() {
        for method in [Method::GET, Method::HEAD, Method::OPTIONS] {
            assert!(!is_state_changing(&method));
        }
        for method in [Method::POST, Method::PUT, Method::PATCH, Method::DELETE] {
            assert!(is_state_changing(&method));
        }
    }

    #[test]
    fn bearer_auth_is_detected() {
        let mut headers = HeaderMap::new();
        assert!(!uses_bearer_auth(&headers));
        headers.insert(
            axum::http::header::AUTHORIZATION,
            "Bearer abc".parse().expect("header"),
        );
        assert!(uses_bearer_auth(&headers));
    }
}
