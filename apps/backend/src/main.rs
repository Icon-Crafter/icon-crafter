//! Server binary: configuration, logging, database, and the Axum listener.

use std::net::SocketAddr;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use iconcrafter_server::auth::session::delete_expired_sessions;
use iconcrafter_server::config::{Config, LogFormat};
use iconcrafter_server::state::AppState;
use iconcrafter_server::{SERVER_VERSION, router};
use sqlx::postgres::PgPoolOptions;
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

/// How often expired sessions are pruned from the database.
const SESSION_CLEANUP_INTERVAL: Duration = Duration::from_secs(15 * 60);
/// Bounded wait when establishing the first database connection.
const DATABASE_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// Bounded wait for a pooled connection at request time.
const DATABASE_ACQUIRE_TIMEOUT: Duration = Duration::from_secs(5);

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("iconcrafter-server: {error}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::from_env()?;
    init_tracing(config.log_format);

    tracing::info!(
        version = SERVER_VERSION,
        listen = %config.listen_addr,
        storage = ?config.storage_kind,
        public_base_url = %config.public_base_url,
        "starting iconcrafter-server"
    );

    let connect = PgPoolOptions::new()
        .max_connections(config.db_max_connections)
        .acquire_timeout(DATABASE_ACQUIRE_TIMEOUT)
        .connect(&config.database_url);
    let pool = tokio::time::timeout(DATABASE_CONNECT_TIMEOUT, connect)
        .await
        .map_err(|_| {
            format!(
                "timed out after {}s connecting to the database configured by DATABASE_URL",
                DATABASE_CONNECT_TIMEOUT.as_secs()
            )
        })?
        .map_err(|error| {
            format!("failed to connect to the database configured by DATABASE_URL: {error}")
        })?;

    sqlx::migrate!("./migrations").run(&pool).await?;
    tracing::info!("database migrations are up to date");

    spawn_session_cleanup(pool.clone());

    let state = AppState::build(Arc::new(config.clone()), pool)?;
    let app = router(state);

    let listener = TcpListener::bind(config.listen_addr)
        .await
        .map_err(|error| format!("failed to bind LISTEN_ADDR {}: {error}", config.listen_addr))?;
    let local_address = listener.local_addr()?;
    tracing::info!(address = %local_address, "listening");

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;

    tracing::info!("server stopped");
    Ok(())
}

/// Installs the tracing subscriber. Honours `RUST_LOG` and switches to
/// newline-delimited JSON when `LOG_FORMAT=json` is configured.
fn init_tracing(format: LogFormat) {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,tower_http=warn,sqlx=warn"));
    let registry = tracing_subscriber::registry().with(filter);
    let installed = match format {
        LogFormat::Json => registry
            .with(tracing_subscriber::fmt::layer().json().flatten_event(true))
            .try_init(),
        LogFormat::Text => registry.with(tracing_subscriber::fmt::layer()).try_init(),
    };
    if installed.is_err() {
        // A subscriber is already installed (only possible when the binary is
        // embedded in another process); keep the existing one.
        eprintln!("iconcrafter-server: tracing subscriber was already installed");
    }
}

/// Periodically deletes expired sessions so the table does not grow without
/// bound. Failures are logged and retried on the next tick.
fn spawn_session_cleanup(pool: sqlx::PgPool) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(SESSION_CLEANUP_INTERVAL);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            interval.tick().await;
            match delete_expired_sessions(&pool).await {
                Ok(0) => {}
                Ok(deleted) => tracing::info!(deleted, "pruned expired sessions"),
                Err(error) => {
                    tracing::warn!(error = %error, "failed to prune expired sessions")
                }
            }
        }
    });
}

/// Resolves on Ctrl-C or, on Unix, SIGTERM.
async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(error) = tokio::signal::ctrl_c().await {
            tracing::warn!(error = %error, "failed to install the Ctrl-C handler");
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut stream) => {
                stream.recv().await;
            }
            Err(error) => {
                tracing::warn!(error = %error, "failed to install the SIGTERM handler");
                std::future::pending::<()>().await;
            }
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {}
        _ = terminate => {}
    }
    tracing::info!("shutdown signal received, draining connections");
}
