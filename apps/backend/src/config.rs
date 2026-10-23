//! Process configuration.
//!
//! Every setting comes from an environment variable. Missing optional variables
//! fall back to safe local-development defaults; missing *required* variables and
//! invalid values produce a [`ConfigError`] that names the offending variable so
//! the process can fail fast with an actionable message instead of starting in a
//! half-configured state.
//!
//! The full list of variables is documented in `apps/backend/README.md` and
//! `apps/backend/.env.example`.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use thiserror::Error;

/// Address used when `LISTEN_ADDR` is not set.
pub const DEFAULT_LISTEN_ADDR: &str = "127.0.0.1:8080";
/// Session cookie name used when `SESSION_COOKIE_NAME` is not set.
pub const DEFAULT_SESSION_COOKIE_NAME: &str = "ic_session";
/// Session lifetime used when `SESSION_TTL_HOURS` is not set (30 days).
pub const DEFAULT_SESSION_TTL_HOURS: u64 = 24 * 30;
/// Upload cap used when `MAX_UPLOAD_BYTES` is not set (25 MiB).
pub const DEFAULT_MAX_UPLOAD_BYTES: u64 = 25 * 1024 * 1024;
/// Default local storage directory.
pub const DEFAULT_STORAGE_LOCAL_DIR: &str = "./data/storage";
/// Hard limit for JSON request bodies (8 MiB). Documents larger than this must
/// be stored through the version/asset pipeline rather than inline JSON.
pub const JSON_BODY_LIMIT_BYTES: usize = 8 * 1024 * 1024;
/// Maximum upload size that can be configured (512 MiB).
pub const MAX_UPLOAD_BYTES_CEILING: u64 = 512 * 1024 * 1024;
/// Default auth rate limit (requests per minute per IP and route).
pub const DEFAULT_RATE_LIMIT_PER_MINUTE: u32 = 60;
/// Deadline applied to the readiness database probe.
pub const READY_DB_TIMEOUT: Duration = Duration::from_secs(2);
/// Upper bound for the number of pooled database connections.
pub const MAX_DB_CONNECTIONS_CEILING: u32 = 100;

/// Errors returned while reading or validating the environment.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ConfigError {
    /// A required variable is absent (or empty).
    #[error("missing required environment variable `{0}`")]
    Missing(&'static str),
    /// A variable is present but its value is not acceptable.
    #[error("invalid value for `{var}`: {reason}")]
    Invalid {
        /// Variable name.
        var: &'static str,
        /// Human readable reason.
        reason: String,
    },
}

impl ConfigError {
    fn invalid(var: &'static str, reason: impl Into<String>) -> Self {
        Self::Invalid {
            var,
            reason: reason.into(),
        }
    }
}

/// Where uploaded assets and published exports are stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageKind {
    /// Local filesystem (always available).
    Local,
    /// S3-compatible object storage (requires the `s3` cargo feature).
    S3,
}

impl StorageKind {
    /// Parse the `STORAGE_KIND` value.
    pub fn parse(raw: &str) -> Result<Self, ConfigError> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "local" => Ok(Self::Local),
            "s3" => Ok(Self::S3),
            other => Err(ConfigError::invalid(
                "STORAGE_KIND",
                format!("expected `local` or `s3`, got `{other}`"),
            )),
        }
    }
}

/// S3 connection settings (only used when [`StorageKind::S3`] is selected).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct S3Config {
    /// Bucket name.
    pub bucket: String,
    /// Region string passed to the AWS SDK (MinIO accepts any non-empty region).
    pub region: String,
    /// Optional custom endpoint, e.g. `http://127.0.0.1:9000` for MinIO.
    pub endpoint: Option<String>,
    /// Static access key. `None` means the SDK default credential chain.
    pub access_key_id: Option<String>,
    /// Static secret key.
    pub secret_access_key: Option<String>,
    /// Use path-style addressing (`endpoint/bucket/key`) instead of virtual hosts.
    pub force_path_style: bool,
    /// Key prefix applied to every object written by this server.
    pub prefix: String,
}

/// Log output format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogFormat {
    /// Human readable single-line logs.
    Text,
    /// Newline-delimited JSON logs.
    Json,
}

impl LogFormat {
    /// Parse the `LOG_FORMAT` value.
    pub fn parse(raw: &str) -> Result<Self, ConfigError> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "text" | "plain" | "console" => Ok(Self::Text),
            "json" => Ok(Self::Json),
            other => Err(ConfigError::invalid(
                "LOG_FORMAT",
                format!("expected `text` or `json`, got `{other}`"),
            )),
        }
    }
}

/// Validated process configuration.
#[derive(Debug, Clone)]
pub struct Config {
    /// PostgreSQL connection string.
    pub database_url: String,
    /// TCP address the HTTP server binds to.
    pub listen_addr: SocketAddr,
    /// Name of the session cookie.
    pub session_cookie_name: String,
    /// Name of the CSRF cookie (non-HttpOnly double-submit cookie).
    pub csrf_cookie_name: String,
    /// Session lifetime.
    pub session_ttl: Duration,
    /// Maximum accepted asset upload size.
    pub max_upload_bytes: usize,
    /// Storage backend selector.
    pub storage_kind: StorageKind,
    /// Root directory for [`StorageKind::Local`].
    pub storage_local_dir: PathBuf,
    /// S3 settings for [`StorageKind::S3`].
    pub s3: Option<S3Config>,
    /// Public base URL used for share/publish links (no trailing slash).
    pub public_base_url: String,
    /// Allowed browser origins. Empty means same-origin only.
    pub cors_origins: Vec<String>,
    /// Whether cookies carry the `Secure` attribute.
    pub cookie_secure: bool,
    /// Auth rate limit (requests per minute per IP + route).
    pub rate_limit_per_minute: u32,
    /// Log output format.
    pub log_format: LogFormat,
    /// Optional directory served read-only under `/static`.
    pub serve_static_dir: Option<PathBuf>,
    /// Maximum pooled database connections.
    pub db_max_connections: u32,
}

impl Config {
    /// Read configuration from the process environment.
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_map(std::env::vars())
    }

    /// Read configuration from an arbitrary key/value source.
    ///
    /// Keys are matched case-sensitively (environment variable names are
    /// conventionally upper case). Blank values are treated as absent.
    pub fn from_map<I, K, V>(vars: I) -> Result<Self, ConfigError>
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        let map: HashMap<String, String> = vars
            .into_iter()
            .map(|(k, v)| (k.into(), v.into()))
            .collect();

        let get = |name: &str| -> Option<&str> {
            map.get(name).map(String::as_str).filter(|v| !v.trim().is_empty())
        };

        let database_url = get("DATABASE_URL")
            .ok_or(ConfigError::Missing("DATABASE_URL"))?
            .trim()
            .to_owned();
        if !(database_url.starts_with("postgres://") || database_url.starts_with("postgresql://")) {
            return Err(ConfigError::invalid(
                "DATABASE_URL",
                "expected a postgres:// or postgresql:// connection URL",
            ));
        }

        let listen_addr = match get("LISTEN_ADDR") {
            None => DEFAULT_LISTEN_ADDR
                .parse::<SocketAddr>()
                .map_err(|e| ConfigError::invalid("LISTEN_ADDR", e.to_string()))?,
            Some(raw) => raw.trim().parse::<SocketAddr>().map_err(|e| {
                ConfigError::invalid(
                    "LISTEN_ADDR",
                    format!("expected `IP:PORT`, got `{raw}` ({e})"),
                )
            })?,
        };

        let session_cookie_name = get("SESSION_COOKIE_NAME")
            .unwrap_or(DEFAULT_SESSION_COOKIE_NAME)
            .trim()
            .to_owned();
        validate_cookie_name("SESSION_COOKIE_NAME", &session_cookie_name)?;
        if session_cookie_name.len() > 48 {
            return Err(ConfigError::invalid(
                "SESSION_COOKIE_NAME",
                "must be at most 48 characters so the derived CSRF cookie name fits in 64 characters",
            ));
        }
        let csrf_cookie_name = format!("{session_cookie_name}_csrf");

        let session_ttl_hours = parse_u64(get("SESSION_TTL_HOURS"), "SESSION_TTL_HOURS")?
            .unwrap_or(DEFAULT_SESSION_TTL_HOURS);
        if !(1..=24 * 365).contains(&session_ttl_hours) {
            return Err(ConfigError::invalid(
                "SESSION_TTL_HOURS",
                "must be between 1 and 8760 hours",
            ));
        }

        let max_upload_bytes = parse_u64(get("MAX_UPLOAD_BYTES"), "MAX_UPLOAD_BYTES")?
            .unwrap_or(DEFAULT_MAX_UPLOAD_BYTES);
        if !(1024..=MAX_UPLOAD_BYTES_CEILING).contains(&max_upload_bytes) {
            return Err(ConfigError::invalid(
                "MAX_UPLOAD_BYTES",
                format!(
                    "must be between 1024 and {MAX_UPLOAD_BYTES_CEILING} bytes (512 MiB)"
                ),
            ));
        }

        let storage_kind = match get("STORAGE_KIND") {
            None => StorageKind::Local,
            Some(raw) => StorageKind::parse(raw)?,
        };
        if storage_kind == StorageKind::S3 && !cfg!(feature = "s3") {
            return Err(ConfigError::invalid(
                "STORAGE_KIND",
                "`s3` requires building the server with `--features s3`",
            ));
        }

        let storage_local_dir = PathBuf::from(
            get("STORAGE_LOCAL_DIR")
                .unwrap_or(DEFAULT_STORAGE_LOCAL_DIR)
                .trim(),
        );
        if storage_local_dir.as_os_str().is_empty() {
            return Err(ConfigError::invalid(
                "STORAGE_LOCAL_DIR",
                "must not be empty",
            ));
        }

        let s3 = if storage_kind == StorageKind::S3 {
            Some(parse_s3_config(&map)?)
        } else {
            None
        };

        let public_base_url = match get("PUBLIC_BASE_URL") {
            None => "http://127.0.0.1:8080".to_owned(),
            Some(raw) => normalize_public_base_url(raw)?,
        };

        let cors_origins = match get("CORS_ORIGINS") {
            None => Vec::new(),
            Some(raw) => parse_cors_origins(raw)?,
        };

        let cookie_secure = match get("COOKIE_SECURE") {
            Some(raw) => parse_bool("COOKIE_SECURE", raw)?,
            // Default to the scheme of the public URL: https implies Secure
            // cookies, plain http (local development) does not.
            None => public_base_url.starts_with("https://"),
        };
        if cookie_secure == false
            && session_cookie_name.starts_with("__Host-")
        {
            return Err(ConfigError::invalid(
                "SESSION_COOKIE_NAME",
                "`__Host-` cookie names require COOKIE_SECURE=true",
            ));
        }

        let rate_limit_per_minute =
            parse_u32(get("RATE_LIMIT_PER_MINUTE"), "RATE_LIMIT_PER_MINUTE")?
                .unwrap_or(DEFAULT_RATE_LIMIT_PER_MINUTE);
        if !(1..=100_000).contains(&rate_limit_per_minute) {
            return Err(ConfigError::invalid(
                "RATE_LIMIT_PER_MINUTE",
                "must be between 1 and 100000",
            ));
        }

        let log_format = match get("LOG_FORMAT") {
            None => LogFormat::Text,
            Some(raw) => LogFormat::parse(raw)?,
        };

        let serve_static_dir = get("SERVE_STATIC_DIR").map(|raw| {
            let path = PathBuf::from(raw.trim());
            path
        });

        let db_max_connections =
            parse_u32(get("DATABASE_MAX_CONNECTIONS"), "DATABASE_MAX_CONNECTIONS")?
                .unwrap_or(10);
        if !(1..=MAX_DB_CONNECTIONS_CEILING).contains(&db_max_connections) {
            return Err(ConfigError::invalid(
                "DATABASE_MAX_CONNECTIONS",
                format!("must be between 1 and {MAX_DB_CONNECTIONS_CEILING}"),
            ));
        }

        Ok(Self {
            database_url,
            listen_addr,
            session_cookie_name,
            csrf_cookie_name,
            session_ttl: Duration::from_secs(session_ttl_hours * 3600),
            max_upload_bytes: max_upload_bytes as usize,
            storage_kind,
            storage_local_dir,
            s3,
            public_base_url,
            cors_origins,
            cookie_secure,
            rate_limit_per_minute,
            log_format,
            serve_static_dir,
            db_max_connections,
        })
    }
}

fn parse_s3_config(map: &HashMap<String, String>) -> Result<S3Config, ConfigError> {
    let get = |name: &str| -> Option<&str> {
        map.get(name).map(String::as_str).filter(|v| !v.trim().is_empty())
    };

    let bucket = get("S3_BUCKET")
        .ok_or(ConfigError::Missing("S3_BUCKET"))?
        .trim()
        .to_owned();
    if bucket.len() > 255 || bucket.contains('/') {
        return Err(ConfigError::invalid(
            "S3_BUCKET",
            "must be a bucket name without slashes",
        ));
    }
    let region = get("S3_REGION")
        .ok_or(ConfigError::Missing("S3_REGION"))?
        .trim()
        .to_owned();

    let endpoint = match get("S3_ENDPOINT") {
        None => None,
        Some(raw) => {
            let url = url::Url::parse(raw.trim()).map_err(|e| {
                ConfigError::invalid("S3_ENDPOINT", format!("expected an http(s) URL: {e}"))
            })?;
            if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
                return Err(ConfigError::invalid(
                    "S3_ENDPOINT",
                    "expected an absolute http(s) URL with a host",
                ));
            }
            Some(url.as_str().trim_end_matches('/').to_owned())
        }
    };

    let access_key_id = get("S3_ACCESS_KEY_ID").map(|v| v.trim().to_owned());
    let secret_access_key = get("S3_SECRET_ACCESS_KEY").map(|v| v.trim().to_owned());
    match (&access_key_id, &secret_access_key) {
        (Some(_), None) => {
            return Err(ConfigError::Missing("S3_SECRET_ACCESS_KEY"));
        }
        (None, Some(_)) => {
            return Err(ConfigError::Missing("S3_ACCESS_KEY_ID"));
        }
        _ => {}
    }

    let force_path_style = match get("S3_FORCE_PATH_STYLE") {
        Some(raw) => parse_bool("S3_FORCE_PATH_STYLE", raw)?,
        None => endpoint.is_some(),
    };

    let prefix = get("S3_PREFIX").unwrap_or("iconcrafter/").trim().to_owned();
    if prefix.len() > 128 || prefix.starts_with('/') || prefix.contains("..") {
        return Err(ConfigError::invalid(
            "S3_PREFIX",
            "must be a relative key prefix without `..` and at most 128 characters",
        ));
    }
    let prefix = if prefix.is_empty() || prefix.ends_with('/') {
        prefix
    } else {
        format!("{prefix}/")
    };

    Ok(S3Config {
        bucket,
        region,
        endpoint,
        access_key_id,
        secret_access_key,
        force_path_style,
        prefix,
    })
}

fn validate_cookie_name(var: &'static str, name: &str) -> Result<(), ConfigError> {
    if name.is_empty() || name.len() > 64 {
        return Err(ConfigError::invalid(var, "must be 1-64 characters"));
    }
    let valid = name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        && !name.chars().next().is_some_and(|c| c.is_ascii_digit());
    if !valid {
        return Err(ConfigError::invalid(
            var,
            "may only contain ASCII letters, digits, `-`, `_` and `.`, and must not start with a digit",
        ));
    }
    Ok(())
}

fn normalize_public_base_url(raw: &str) -> Result<String, ConfigError> {
    let url = url::Url::parse(raw.trim()).map_err(|e| {
        ConfigError::invalid(
            "PUBLIC_BASE_URL",
            format!("expected an absolute http(s) URL: {e}"),
        )
    })?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(ConfigError::invalid(
            "PUBLIC_BASE_URL",
            "expected an http or https URL",
        ));
    }
    if url.host_str().is_none() {
        return Err(ConfigError::invalid(
            "PUBLIC_BASE_URL",
            "expected a URL with a host",
        ));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(ConfigError::invalid(
            "PUBLIC_BASE_URL",
            "must not contain credentials",
        ));
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err(ConfigError::invalid(
            "PUBLIC_BASE_URL",
            "must not contain a query string or fragment",
        ));
    }
    Ok(url.as_str().trim_end_matches('/').to_owned())
}

fn parse_cors_origins(raw: &str) -> Result<Vec<String>, ConfigError> {
    let mut origins = Vec::new();
    for entry in raw.split(',') {
        let entry = entry.trim();
        if entry.is_empty() {
            continue;
        }
        let url = url::Url::parse(entry).map_err(|e| {
            ConfigError::invalid("CORS_ORIGINS", format!("invalid origin `{entry}`: {e}"))
        })?;
        if !matches!(url.scheme(), "http" | "https") {
            return Err(ConfigError::invalid(
                "CORS_ORIGINS",
                format!("origin `{entry}` must use http or https"),
            ));
        }
        if url.host_str().is_none() {
            return Err(ConfigError::invalid(
                "CORS_ORIGINS",
                format!("origin `{entry}` must contain a host"),
            ));
        }
        if url.path() != "/" && !url.path().is_empty() {
            return Err(ConfigError::invalid(
                "CORS_ORIGINS",
                format!("origin `{entry}` must not contain a path"),
            ));
        }
        if url.query().is_some()
            || url.fragment().is_some()
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return Err(ConfigError::invalid(
                "CORS_ORIGINS",
                format!("origin `{entry}` must be a bare origin"),
            ));
        }
        let normalized = format!(
            "{}://{}{}",
            url.scheme(),
            url.host_str().unwrap_or_default(),
            match url.port() {
                Some(port) => format!(":{port}"),
                None => String::new(),
            }
        );
        if !origins.contains(&normalized) {
            origins.push(normalized);
        }
    }
    Ok(origins)
}

fn parse_bool(var: &'static str, raw: &str) -> Result<bool, ConfigError> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        other => Err(ConfigError::invalid(
            var,
            format!("expected a boolean (`true`/`false`), got `{other}`"),
        )),
    }
}

fn parse_u64(raw: Option<&str>, var: &'static str) -> Result<Option<u64>, ConfigError> {
    match raw {
        None => Ok(None),
        Some(value) => value
            .trim()
            .parse::<u64>()
            .map(Some)
            .map_err(|e| ConfigError::invalid(var, format!("expected an integer: {e}"))),
    }
}

fn parse_u32(raw: Option<&str>, var: &'static str) -> Result<Option<u32>, ConfigError> {
    match raw {
        None => Ok(None),
        Some(value) => value
            .trim()
            .parse::<u32>()
            .map(Some)
            .map_err(|e| ConfigError::invalid(var, format!("expected an integer: {e}"))),
    }
}

/// Convenience constructor for tests and tools.
#[cfg(test)]
pub(crate) fn test_config() -> Config {
    Config::from_map([
        ("DATABASE_URL", "postgres://postgres:postgres@127.0.0.1:5432/iconcrafter"),
        ("COOKIE_SECURE", "false"),
    ])
    .expect("test configuration is valid")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> HashMap<String, String> {
        let mut map = HashMap::new();
        map.insert(
            "DATABASE_URL".to_owned(),
            "postgres://user:pass@localhost:5432/iconcrafter".to_owned(),
        );
        map
    }

    #[test]
    fn defaults_are_applied() {
        let cfg = Config::from_map(base()).expect("valid config");
        assert_eq!(cfg.listen_addr, DEFAULT_LISTEN_ADDR.parse().unwrap());
        assert_eq!(cfg.session_cookie_name, DEFAULT_SESSION_COOKIE_NAME);
        assert_eq!(cfg.csrf_cookie_name, "ic_session_csrf");
        assert_eq!(cfg.session_ttl, Duration::from_secs(30 * 24 * 3600));
        assert_eq!(cfg.max_upload_bytes, 25 * 1024 * 1024);
        assert_eq!(cfg.storage_kind, StorageKind::Local);
        assert_eq!(cfg.storage_local_dir, PathBuf::from(DEFAULT_STORAGE_LOCAL_DIR));
        assert!(cfg.s3.is_none());
        assert_eq!(cfg.public_base_url, "http://127.0.0.1:8080");
        assert!(cfg.cors_origins.is_empty());
        assert!(!cfg.cookie_secure);
        assert_eq!(cfg.rate_limit_per_minute, DEFAULT_RATE_LIMIT_PER_MINUTE);
        assert_eq!(cfg.log_format, LogFormat::Text);
        assert_eq!(cfg.db_max_connections, 10);
    }

    #[test]
    fn missing_database_url_is_reported() {
        let err = Config::from_map(Vec::<(String, String)>::new()).unwrap_err();
        assert_eq!(err, ConfigError::Missing("DATABASE_URL"));
        assert!(err.to_string().contains("DATABASE_URL"));
    }

    #[test]
    fn blank_database_url_is_treated_as_missing() {
        let err = Config::from_map([("DATABASE_URL", "   ")]).unwrap_err();
        assert_eq!(err, ConfigError::Missing("DATABASE_URL"));
    }

    #[test]
    fn non_postgres_database_url_is_rejected() {
        let err = Config::from_map([("DATABASE_URL", "mysql://x/y")]).unwrap_err();
        assert_eq!(
            err,
            ConfigError::invalid("DATABASE_URL", "expected a postgres:// or postgresql:// connection URL")
        );
    }

    #[test]
    fn listen_addr_parsing() {
        let mut map = base();
        map.insert("LISTEN_ADDR".into(), "0.0.0.0:9000".into());
        let cfg = Config::from_map(map).unwrap();
        assert_eq!(cfg.listen_addr, "0.0.0.0:9000".parse().unwrap());

        let mut map = base();
        map.insert("LISTEN_ADDR".into(), "not-an-address".into());
        let err = Config::from_map(map).unwrap_err();
        assert!(matches!(err, ConfigError::Invalid { var: "LISTEN_ADDR", .. }));
    }

    #[test]
    fn cookie_name_validation() {
        let mut map = base();
        map.insert("SESSION_COOKIE_NAME".into(), "bad name!".into());
        assert!(Config::from_map(map).is_err());

        let mut map = base();
        map.insert("SESSION_COOKIE_NAME".into(), "__Host-ic".into());
        let err = Config::from_map(map).unwrap_err();
        assert!(err.to_string().contains("COOKIE_SECURE"));

        let mut map = base();
        map.insert("SESSION_COOKIE_NAME".into(), "__Host-ic".into());
        map.insert("COOKIE_SECURE".into(), "true".into());
        assert!(Config::from_map(map).is_ok());
    }

    #[test]
    fn session_ttl_bounds() {
        let mut map = base();
        map.insert("SESSION_TTL_HOURS".into(), "0".into());
        assert!(Config::from_map(map).is_err());

        let mut map = base();
        map.insert("SESSION_TTL_HOURS".into(), "48".into());
        let cfg = Config::from_map(map).unwrap();
        assert_eq!(cfg.session_ttl, Duration::from_secs(48 * 3600));
    }

    #[test]
    fn upload_bounds() {
        let mut map = base();
        map.insert("MAX_UPLOAD_BYTES".into(), "512".into());
        assert!(Config::from_map(map).is_err());

        let mut map = base();
        map.insert("MAX_UPLOAD_BYTES".into(), "1048576".into());
        let cfg = Config::from_map(map).unwrap();
        assert_eq!(cfg.max_upload_bytes, 1_048_576);
    }

    #[test]
    fn storage_kind_requires_s3_settings() {
        let mut map = base();
        map.insert("STORAGE_KIND".into(), "s3".into());
        let err = Config::from_map(map).unwrap_err();
        // Without the `s3` feature the first error is that the build lacks it.
        if cfg!(feature = "s3") {
            assert_eq!(err, ConfigError::Missing("S3_BUCKET"));
        } else {
            assert!(err.to_string().contains("--features s3"));
        }
    }

    #[test]
    fn s3_settings_are_parsed() {
        if !cfg!(feature = "s3") {
            return;
        }
        let mut map = base();
        map.insert("STORAGE_KIND".into(), "s3".into());
        map.insert("S3_BUCKET".into(), "iconcrafter".into());
        map.insert("S3_REGION".into(), "us-east-1".into());
        map.insert("S3_ENDPOINT".into(), "http://127.0.0.1:9000/".into());
        map.insert("S3_ACCESS_KEY_ID".into(), "minio".into());
        map.insert("S3_SECRET_ACCESS_KEY".into(), "minio-secret".into());
        let cfg = Config::from_map(map).unwrap();
        let s3 = cfg.s3.expect("s3 config");
        assert_eq!(s3.bucket, "iconcrafter");
        assert_eq!(s3.endpoint.as_deref(), Some("http://127.0.0.1:9000"));
        assert!(s3.force_path_style);
        assert_eq!(s3.prefix, "iconcrafter/");
    }

    #[test]
    fn s3_credentials_must_be_paired() {
        if !cfg!(feature = "s3") {
            return;
        }
        let mut map = base();
        map.insert("STORAGE_KIND".into(), "s3".into());
        map.insert("S3_BUCKET".into(), "iconcrafter".into());
        map.insert("S3_REGION".into(), "us-east-1".into());
        map.insert("S3_ACCESS_KEY_ID".into(), "minio".into());
        let err = Config::from_map(map).unwrap_err();
        assert_eq!(err, ConfigError::Missing("S3_SECRET_ACCESS_KEY"));
    }

    #[test]
    fn public_base_url_is_normalized() {
        let mut map = base();
        map.insert("PUBLIC_BASE_URL".into(), "https://design.example.com/".into());
        let cfg = Config::from_map(map).unwrap();
        assert_eq!(cfg.public_base_url, "https://design.example.com");
        // https implies secure cookies by default.
        assert!(cfg.cookie_secure);
    }

    #[test]
    fn public_base_url_rejects_query() {
        let mut map = base();
        map.insert("PUBLIC_BASE_URL".into(), "https://x.test/?a=1".into());
        assert!(Config::from_map(map).is_err());
    }

    #[test]
    fn cors_origins_are_normalized_and_deduplicated() {
        let mut map = base();
        map.insert(
            "CORS_ORIGINS".into(),
            "https://app.example.com, https://app.example.com/,http://localhost:5173".into(),
        );
        let cfg = Config::from_map(map).unwrap();
        assert_eq!(
            cfg.cors_origins,
            vec![
                "https://app.example.com".to_owned(),
                "http://localhost:5173".to_owned()
            ]
        );
    }

    #[test]
    fn cors_origin_with_path_is_rejected() {
        let mut map = base();
        map.insert("CORS_ORIGINS".into(), "https://app.example.com/path".into());
        assert!(Config::from_map(map).is_err());
    }

    #[test]
    fn boolean_parsing() {
        for value in ["1", "true", "YES", "On"] {
            let mut map = base();
            map.insert("COOKIE_SECURE".into(), value.into());
            assert!(Config::from_map(map).unwrap().cookie_secure, "{value}");
        }
        for value in ["0", "false", "no", "OFF"] {
            let mut map = base();
            map.insert("COOKIE_SECURE".into(), value.into());
            assert!(!Config::from_map(map).unwrap().cookie_secure, "{value}");
        }
        let mut map = base();
        map.insert("COOKIE_SECURE".into(), "maybe".into());
        assert!(Config::from_map(map).is_err());
    }

    #[test]
    fn rate_limit_bounds() {
        let mut map = base();
        map.insert("RATE_LIMIT_PER_MINUTE".into(), "0".into());
        assert!(Config::from_map(map).is_err());

        let mut map = base();
        map.insert("RATE_LIMIT_PER_MINUTE".into(), "300".into());
        assert_eq!(
            Config::from_map(map).unwrap().rate_limit_per_minute,
            300
        );
    }

    #[test]
    fn log_format_parsing() {
        let mut map = base();
        map.insert("LOG_FORMAT".into(), "json".into());
        assert_eq!(Config::from_map(map).unwrap().log_format, LogFormat::Json);

        let mut map = base();
        map.insert("LOG_FORMAT".into(), "yaml".into());
        assert!(Config::from_map(map).is_err());
    }

    #[test]
    fn db_connection_bounds() {
        let mut map = base();
        map.insert("DATABASE_MAX_CONNECTIONS".into(), "0".into());
        assert!(Config::from_map(map).is_err());
    }
}
