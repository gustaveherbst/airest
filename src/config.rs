pub mod production;

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::llm::ProviderConfig;

pub const DEFAULT_PORT: u16 = 3300;

#[derive(Debug, Clone)]
pub struct Config {
    pub port: u16,
    pub api_key: Option<String>,
    pub providers: ProviderConfig,
    pub api_dir: PathBuf,
    pub log_level: String,
    pub hot_reload: bool,
    pub load_recursive: bool,
    pub otel_enabled: bool,
    pub otel_metrics: bool,
    pub otel_sample_ratio: f64,
    pub otel_endpoint: Option<String>,
    pub jwt_jwks_url: Option<String>,
    pub jwt_issuer: Option<String>,
    pub jwt_audience: Option<String>,
    pub oauth2_introspection_url: Option<String>,
    pub oauth2_client_id: Option<String>,
    pub oauth2_client_secret: Option<String>,
    pub cache_enabled: bool,
    pub cache_store_path: PathBuf,
    pub cache_max_entries: usize,
    pub cache_embedder_provider: String,
    pub cache_embedder_model: String,
    pub jti_denylist: Vec<String>,
    pub redis_url: Option<String>,
    pub jti_denylist_redis_key: String,
    pub production_mode: bool,
    pub max_request_body_bytes: usize,
    pub request_timeout_secs: u64,
    pub graceful_shutdown_enabled: bool,
    pub graceful_shutdown_secs: u64,
    pub max_concurrent_requests: usize,
    pub llm_circuit_breaker_threshold: u32,
    pub llm_circuit_breaker_reset_secs: u64,
}

/// Default definitions directory when `AIREST_API_DIR` is not set: the process working directory.
pub const DEFAULT_API_DIR: &str = ".";

pub fn load_env(env_file: Option<&Path>) -> Result<()> {
    if let Some(path) = env_file {
        dotenvy::from_path(path)
            .with_context(|| format!("Failed to load env file: {}", path.display()))?;
        return Ok(());
    }

    if let Ok(cwd) = std::env::current_dir() {
        let env_path = cwd.join(".env");
        if env_path.is_file() {
            let _ = dotenvy::from_path(&env_path);
            return Ok(());
        }
    }

    dotenvy::dotenv().ok();
    Ok(())
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Self::load()
    }

    pub fn from_env_cli() -> Result<Self> {
        Self::load()
    }

    fn load() -> Result<Self> {
        let port = std::env::var("AIREST_PORT")
            .unwrap_or_else(|_| DEFAULT_PORT.to_string())
            .parse()
            .context("AIREST_PORT must be a valid port number")?;

        let hot_reload = std::env::var("AIREST_HOT_RELOAD")
            .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
            .unwrap_or(false);

        let load_recursive = std::env::var("AIREST_LOAD_RECURSIVE")
            .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
            .unwrap_or(true);

        let otel_enabled = std::env::var("AIREST_OTEL_ENABLED")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);

        let otel_metrics = std::env::var("AIREST_OTEL_METRICS")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);

        let otel_sample_ratio = std::env::var("AIREST_OTEL_SAMPLE_RATIO")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(1.0);

        let cache_enabled = std::env::var("AIREST_CACHE_ENABLED")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);

        let cache_store_path = std::env::var("AIREST_CACHE_STORE_PATH")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .unwrap_or_default();

        let cache_max_entries = std::env::var("AIREST_CACHE_MAX_ENTRIES")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(crate::cache::DEFAULT_MAX_ENTRIES_PER_SCOPE);

        let cache_embedder_provider = std::env::var("AIREST_CACHE_EMBED_PROVIDER")
            .unwrap_or_else(|_| "hash".to_string());

        let cache_embedder_model = std::env::var("AIREST_CACHE_EMBED_MODEL")
            .unwrap_or_else(|_| "text-embedding-3-small".to_string());

        let jti_denylist = std::env::var("AIREST_JTI_DENYLIST")
            .ok()
            .map(|value| {
                value
                    .split(',')
                    .map(str::trim)
                    .filter(|item| !item.is_empty())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();

        let redis_url = read_optional_env("AIREST_REDIS_URL");
        let jti_denylist_redis_key = std::env::var("AIREST_JTI_DENYLIST_KEY")
            .unwrap_or_else(|_| "airest:jti:denylist".to_string());

        let production_mode = std::env::var("AIREST_PRODUCTION")
            .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
            .unwrap_or(false);

        let max_request_body_bytes = std::env::var("AIREST_MAX_REQUEST_BODY_BYTES")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(1_048_576);

        let request_timeout_secs = std::env::var("AIREST_REQUEST_TIMEOUT_SECS")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(120);

        let graceful_shutdown_enabled = std::env::var("AIREST_GRACEFUL_SHUTDOWN")
            .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
            .unwrap_or(false);

        let graceful_shutdown_secs = std::env::var("AIREST_GRACEFUL_SHUTDOWN_SECS")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(30);

        let max_concurrent_requests = std::env::var("AIREST_MAX_CONCURRENT_REQUESTS")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(64);

        let llm_circuit_breaker_threshold = std::env::var("AIREST_LLM_CIRCUIT_BREAKER_THRESHOLD")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(5);

        let llm_circuit_breaker_reset_secs =
            std::env::var("AIREST_LLM_CIRCUIT_BREAKER_RESET_SECS")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(30);

        Ok(Self {
            port,
            api_key: read_optional_env("AIREST_API_KEY"),
            providers: ProviderConfig::from_env(),
            api_dir: PathBuf::from(
                std::env::var("AIREST_API_DIR").unwrap_or_else(|_| DEFAULT_API_DIR.to_string()),
            ),
            log_level: std::env::var("AIREST_LOG_LEVEL").unwrap_or_else(|_| "info".to_string()),
            hot_reload,
            load_recursive,
            otel_enabled,
            otel_metrics,
            otel_sample_ratio,
            otel_endpoint: read_optional_env("OTEL_EXPORTER_OTLP_ENDPOINT"),
            jwt_jwks_url: read_optional_env("AIREST_JWT_JWKS_URL"),
            jwt_issuer: read_optional_env("AIREST_JWT_ISSUER"),
            jwt_audience: read_optional_env("AIREST_JWT_AUDIENCE"),
            oauth2_introspection_url: read_optional_env("AIREST_OAUTH2_INTROSPECTION_URL"),
            oauth2_client_id: read_optional_env("AIREST_OAUTH2_CLIENT_ID"),
            oauth2_client_secret: read_optional_env("AIREST_OAUTH2_CLIENT_SECRET"),
            cache_enabled,
            cache_store_path,
            cache_max_entries,
            cache_embedder_provider,
            cache_embedder_model,
            jti_denylist,
            redis_url,
            jti_denylist_redis_key,
            production_mode,
            max_request_body_bytes,
            request_timeout_secs,
            graceful_shutdown_enabled,
            graceful_shutdown_secs,
            max_concurrent_requests,
            llm_circuit_breaker_threshold,
            llm_circuit_breaker_reset_secs,
        })
    }

    pub fn api_key(&self) -> Option<&str> {
        self.api_key
            .as_deref()
            .filter(|value| !value.trim().is_empty())
    }

    pub fn jwt_jwks_url(&self) -> Option<&str> {
        self.jwt_jwks_url.as_deref()
    }

    pub fn jwt_issuer(&self) -> Option<&str> {
        self.jwt_issuer.as_deref()
    }

    pub fn jwt_audience(&self) -> Option<&str> {
        self.jwt_audience.as_deref()
    }

    pub fn oauth2_introspection_url(&self) -> Option<&str> {
        self.oauth2_introspection_url.as_deref()
    }

    pub fn oauth2_client_id(&self) -> Option<&str> {
        self.oauth2_client_id.as_deref()
    }

    pub fn oauth2_client_secret(&self) -> Option<&str> {
        self.oauth2_client_secret.as_deref()
    }

    /// Builds a test configuration with optional API key override.
    pub fn for_test(api_key: Option<String>, api_dir: PathBuf) -> Self {
        Self {
            port: DEFAULT_PORT,
            api_key,
            providers: ProviderConfig::from_env(),
            api_dir,
            log_level: "error".to_string(),
            hot_reload: false,
            load_recursive: true,
            otel_enabled: false,
            otel_metrics: false,
            otel_sample_ratio: 1.0,
            otel_endpoint: None,
            jwt_jwks_url: None,
            jwt_issuer: None,
            jwt_audience: None,
            oauth2_introspection_url: None,
            oauth2_client_id: None,
            oauth2_client_secret: None,
            cache_enabled: false,
            cache_store_path: PathBuf::new(),
            cache_max_entries: crate::cache::DEFAULT_MAX_ENTRIES_PER_SCOPE,
            cache_embedder_provider: "hash".to_string(),
            cache_embedder_model: "text-embedding-3-small".to_string(),
            jti_denylist: Vec::new(),
            redis_url: None,
            jti_denylist_redis_key: "airest:jti:denylist".to_string(),
            production_mode: false,
            max_request_body_bytes: 1_048_576,
            request_timeout_secs: 120,
            graceful_shutdown_enabled: false,
            graceful_shutdown_secs: 30,
            max_concurrent_requests: 64,
            llm_circuit_breaker_threshold: 5,
            llm_circuit_breaker_reset_secs: 30,
        }
    }
}

fn read_optional_env(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}
