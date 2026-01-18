use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub storage: StorageConfig,
    pub streaming: StreamingConfig,
    #[serde(default)]
    pub cors: CorsConfig,
    pub admin: AdminConfig,
    pub logging: LoggingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Listen host/ip (optional when `--listen` is provided).
    #[serde(default)]
    pub host: Option<String>,
    /// Listen port (optional when `--listen` is provided).
    #[serde(default)]
    pub port: Option<u16>,
    /// Public base URL (optional).
    ///
    /// Used for features that need to know whether we're running behind HTTPS (e.g. setting
    /// Secure cookies). If not set, defaults to empty.
    #[serde(default)]
    pub base_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    pub backend: String, // "local" or "s3"
    pub s3: Option<S3StorageConfig>,
    /// Local root directory containing `metadata/` and `media/`.
    ///
    /// - Metadata directory: `{base_path}/metadata`
    /// - Media directory: `{base_path}/media`
    ///
    /// This is only used when `storage.backend = "local"`.
    pub base_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct S3StorageConfig {
    pub bucket: String,
    pub region: String,
    pub endpoint: String,
    pub access_key_id: Option<String>,
    pub secret_access_key: Option<String>,
    pub public_access_domain: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamingConfig {
    pub time_shift_buffer_depth: f64,
    pub suggested_presentation_delay: f64,
    /// Minimum amount of timeline ahead of "now" to include in the MPD, in seconds.
    pub min_future_manifest_duration: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CorsConfig {
    /// Allowed origins for cross-origin requests.
    ///
    /// If empty, we do not enable CORS at all (same-origin only).
    /// If it contains "*", any origin is allowed.
    #[serde(default)]
    pub allowed_origins: Vec<String>,
}

fn default_admin_rate_limit_per_second() -> u64 {
    5
}

fn default_admin_max_concurrency() -> usize {
    8
}

fn default_admin_session_ttl_seconds() -> u64 {
    // 24 hours; long enough for convenience, short enough to limit exposure if leaked.
    60 * 60 * 24
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminConfig {
    /// Admin username (required).
    #[serde(default)]
    pub username: Option<String>,
    /// Bcrypt password hash (required).
    #[serde(default)]
    pub password_hash: Option<String>,
    /// Requests per second, process-wide, for admin endpoints.
    #[serde(default = "default_admin_rate_limit_per_second")]
    pub rate_limit_per_second: u64,
    /// Max number of concurrent in-flight requests for admin endpoints.
    #[serde(default = "default_admin_max_concurrency")]
    pub max_concurrency: usize,
    /// Session cookie TTL for admin login.
    #[serde(default = "default_admin_session_ttl_seconds")]
    pub session_ttl_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    pub level: String,
}

impl Config {
    /// Load configuration from TOML file
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config = toml::from_str(&content)?;
        Ok(config)
    }
}
