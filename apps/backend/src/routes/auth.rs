//! Authentication endpoints.

use std::net::SocketAddr;

use axum::Json;
use axum::extract::{ConnectInfo, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::password::{hash_password, verify_password};
use crate::auth::session::{create_session, delete_session};
use crate::auth::tokens::{TokenError, generate_token, token_hash};
use crate::auth::{self, AuthUser};
use crate::config::Config;
use crate::error::{ApiError, AppResult};
use crate::models::UserView;
use crate::services::audit;
use crate::state::AppState;
use crate::validation;

/// Registration payload.
#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    /// E-mail address (normalized and validated).
    pub email: String,
    /// Display name.
    pub display_name: String,
    /// Password (Argon2id hashed with a per-password salt).
    pub password: String,
}

/// Login payload.
#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    /// E-mail address.
    pub email: String,
    /// Password.
    pub password: String,
}

/// Session metadata.
#[derive(Debug, Serialize)]
pub struct SessionInfo {
    /// Absolute expiry.
    pub expires_at: DateTime<Utc>,
}

/// Auth response.
#[derive(Debug, Serialize)]
pub struct AuthResponse {
    /// Authenticated user.
    pub user: UserView,
    /// Session metadata.
    pub session: SessionInfo,
}

/// Register a new account and start a session.
pub async fn register(
    State(state): State<AppState>,
    ConnectInfo(connect): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(payload): Json<RegisterRequest>,
) -> AppResult<Response> {
    let ip = auth::client_ip(&headers, Some(connect));
    auth::enforce_rate_limit(&state, &ip, "auth_register")?;

    let email = validation::require_email(&payload.email)?;
    let display_name = validation::require_name(&payload.display_name, "display_name")?;
    validation::require_password(&payload.password)?;

    let password = payload.password;
    let password_hash = tokio::task::spawn_blocking(move || hash_password(&password))
        .await
        .map_err(|err| ApiError::internal_context("password hashing task", err))?
        .map_err(|err| ApiError::internal(format!("password hashing failed: {err}")))?;

    let user: UserView = match sqlx::query_as(
        "INSERT INTO users (email, display_name, password_hash) \
         VALUES ($1, $2, $3) \
         RETURNING id, email, display_name, created_at",
    )
    .bind(&email)
    .bind(&display_name)
    .bind(&password_hash)
    .fetch_one(&state.db)
    .await
    {
        Ok(user) => user,
        Err(sqlx::Error::Database(error)) if error.code().as_deref() == Some("23505") => {
            return Err(ApiError::conflict(
                "an account with this e-mail address already exists",
            ));
        }
        Err(error) => return Err(error.into()),
    };

    let (session_info, cookies) = start_session(&state, &headers, user.id, &ip).await?;

    audit::record_pool(
        &state.db,
        Some(user.id),
        "auth.register",
        "user",
        Some(&user.id.to_string()),
        serde_json::json!({"email": user.email}),
        Some(&ip),
    )
    .await?;

    let mut response = (
        StatusCode::CREATED,
        Json(AuthResponse {
            user,
            session: session_info,
        }),
    )
        .into_response();
    attach_cookies(&mut response, &state.config, &cookies)?;
    Ok(response)
}

/// Authenticate and start a session.
pub async fn login(
    State(state): State<AppState>,
    ConnectInfo(connect): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(payload): Json<LoginRequest>,
) -> AppResult<Response> {
    let ip = auth::client_ip(&headers, Some(connect));
    auth::enforce_rate_limit(&state, &ip, "auth_login")?;

    let email = validation::normalize_email(&payload.email);
    if email.is_empty() || payload.password.is_empty() {
        return Err(ApiError::unauthorized("invalid e-mail or password"));
    }

    let record: Option<(Uuid, String, bool)> =
        sqlx::query_as("SELECT id, password_hash, is_active FROM users WHERE lower(email) = $1")
            .bind(&email)
            .fetch_optional(&state.db)
            .await?;
    let Some((user_id, stored_hash, is_active)) = record else {
        return Err(ApiError::unauthorized("invalid e-mail or password"));
    };
    if !is_active {
        return Err(ApiError::unauthorized("this account is disabled"));
    }

    let password = payload.password;
    let verified = tokio::task::spawn_blocking(move || verify_password(&password, &stored_hash))
        .await
        .map_err(|err| ApiError::internal_context("password verification task", err))?;
    if !verified {
        return Err(ApiError::unauthorized("invalid e-mail or password"));
    }

    let user: UserView =
        sqlx::query_as("SELECT id, email, display_name, created_at FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_one(&state.db)
            .await?;

    let (session_info, cookies) = start_session(&state, &headers, user.id, &ip).await?;

    audit::record_pool(
        &state.db,
        Some(user.id),
        "auth.login",
        "user",
        Some(&user.id.to_string()),
        serde_json::json!({}),
        Some(&ip),
    )
    .await?;

    let mut response = Json(AuthResponse {
        user,
        session: session_info,
    })
    .into_response();
    attach_cookies(&mut response, &state.config, &cookies)?;
    Ok(response)
}

/// End the current session.
pub async fn logout(State(state): State<AppState>, headers: HeaderMap) -> AppResult<Response> {
    if let Some(token) = auth::extract_session_token(&headers, &state.config.session_cookie_name) {
        let hash = token_hash(&token);
        if delete_session(&state.db, &hash).await?
            && let Ok(user) = auth::authenticate_headers(&state, &headers).await
        {
            let ip = auth::client_ip(&headers, None);
            audit::record_pool(
                &state.db,
                Some(user.user_id),
                "auth.logout",
                "session",
                Some(&user.session_id.to_string()),
                serde_json::json!({}),
                Some(&ip),
            )
            .await?;
        }
    }
    let mut response = StatusCode::NO_CONTENT.into_response();
    for cookie in auth::clear_auth_cookies(&state.config) {
        append_set_cookie(&mut response, &cookie)?;
    }
    Ok(response)
}

/// Return the authenticated user and session metadata.
pub async fn me(State(state): State<AppState>, user: AuthUser) -> AppResult<Json<AuthResponse>> {
    let view: UserView =
        sqlx::query_as("SELECT id, email, display_name, created_at FROM users WHERE id = $1")
            .bind(user.user_id)
            .fetch_one(&state.db)
            .await?;
    Ok(Json(AuthResponse {
        user: view,
        session: SessionInfo {
            expires_at: user.expires_at,
        },
    }))
}

struct IssuedCookies {
    session: String,
    csrf: String,
}

async fn start_session(
    state: &AppState,
    headers: &HeaderMap,
    user_id: Uuid,
    ip: &str,
) -> AppResult<(SessionInfo, IssuedCookies)> {
    let token = generate_token().map_err(token_error)?;
    let csrf = generate_token().map_err(token_error)?;
    let hash = token_hash(&token);
    let user_agent = auth::user_agent(headers);
    let session_id = create_session(
        &state.db,
        user_id,
        &hash,
        state.config.session_ttl,
        user_agent.as_deref(),
        Some(ip),
    )
    .await?;
    let expires_at: DateTime<Utc> =
        sqlx::query_scalar("SELECT expires_at FROM sessions WHERE id = $1")
            .bind(session_id)
            .fetch_one(&state.db)
            .await?;
    Ok((
        SessionInfo { expires_at },
        IssuedCookies {
            session: token,
            csrf,
        },
    ))
}

fn token_error(error: TokenError) -> ApiError {
    ApiError::internal(format!("secure token generation failed: {error}"))
}

fn attach_cookies(
    response: &mut Response,
    config: &Config,
    cookies: &IssuedCookies,
) -> AppResult<()> {
    append_set_cookie(response, &auth::session_cookie(config, &cookies.session))?;
    append_set_cookie(response, &auth::csrf_cookie(config, &cookies.csrf))?;
    Ok(())
}

fn append_set_cookie(response: &mut Response, cookie: &str) -> AppResult<()> {
    let value = HeaderValue::from_str(cookie)
        .map_err(|err| ApiError::internal_context("building Set-Cookie header", err))?;
    response.headers_mut().append(header::SET_COOKIE, value);
    Ok(())
}
