use std::{fs, path::Path};
use serde::Deserialize;
use thiserror::Error;

const DEFAULT_HOST: &str = "0.0.0.0";
const DEFAULT_PORT: u16 = 8080;
const DEFAULT_JWKS_ENDPOINT: &str = "https://auth.internal/.well_known/jwks.json";
const DEFAULT_REDIS_URL: &str = "redis://127.0.0.1:6379";
const DEFAULT_MAX_CONTENT_LENGTH: u64 = 10 * 1024 * 1024;
const DEFAULT_TIMEOUT_SECS: u64 = 30;
const DEFAULT_MIN_COOLDOWN_SECS: u64 = 60;
const DEFAULT_MAX_COOLDOWN_SECS: u64 = 240;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("Failed to read config file '{path}': {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("Failed to parse TOML from '{path}': {source}")]
    Parse {
        path: String,
        #[source]
        source: toml::de::Error,
    },
    #[error("Config file not found in searched locations")]
    NotFound,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct GatewayConfig {
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub auth: AuthConfig,
    #[serde(default)]
    pub redis: RedisConfig,
    #[serde(default)]
    pub limits: LimitsConfig,
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            server: ServerConfig::default(),
            auth: AuthConfig::default(),
            redis: RedisConfig::default(),
            limits: LimitsConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct ServerConfig {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
        }
    }
}

fn default_host() -> String {
    DEFAULT_HOST.to_string()
}

fn default_port() -> u16 {
    DEFAULT_PORT
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct AuthConfig {
    #[serde(default = "default_jwks_endpoint")]
    pub jwks_endpoint: String,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            jwks_endpoint: default_jwks_endpoint(),
        }
    }
}

fn default_jwks_endpoint() -> String {
    DEFAULT_JWKS_ENDPOINT.to_string()
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct RedisConfig {
    #[serde(default = "default_redis_url")]
    pub url: String,
}

impl Default for RedisConfig {
    fn default() -> Self {
        Self {
            url: default_redis_url(),
        }
    }
}

fn default_redis_url() -> String {
    DEFAULT_REDIS_URL.to_string()
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct LimitsConfig {
    #[serde(default = "default_max_content_length")]
    pub max_content_length_bytes: u64,
    #[serde(default = "default_request_timeout_secs")]
    pub request_timeout_secs: u64,
    #[serde(default = "default_min_cooldown_secs")]
    pub min_cooldown_secs: u64,
    #[serde(default = "default_max_cooldown_secs")]
    pub max_cooldown_secs: u64,
}

impl Default for LimitsConfig {
    fn default() -> Self {
        Self {
            max_content_length_bytes: default_max_content_length(),
            request_timeout_secs: default_request_timeout_secs(),
            min_cooldown_secs: default_min_cooldown_secs(),
            max_cooldown_secs: default_max_cooldown_secs(),
        }
    }
}

fn default_max_content_length() -> u64 {
    DEFAULT_MAX_CONTENT_LENGTH
}

fn default_request_timeout_secs() -> u64 {
    DEFAULT_TIMEOUT_SECS
}

fn default_min_cooldown_secs() -> u64 {
    DEFAULT_MIN_COOLDOWN_SECS
}

fn default_max_cooldown_secs() -> u64 {
    DEFAULT_MAX_COOLDOWN_SECS
}

impl GatewayConfig {
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, ConfigError> {
        let path_ref = path.as_ref();
        let path_str = path_ref.to_string_lossy().to_string();
        let content = fs::read_to_string(path_ref).map_err(|e| ConfigError::Io {
            path: path_str.clone(),
            source: e,
        })?;
        let config: GatewayConfig = toml::from_str(&content).map_err(|e| ConfigError::Parse {
            path: path_str,
            source: e,
        })?;
        Ok(config)
    }

    pub fn load() -> Result<Self, ConfigError> {
        if let Ok(custom_path) = std::env::var("API_GATEWAY_CONFIG") {
            return Self::from_file(custom_path);
        }

        let candidates = [
            "api-gateway.toml",
            "../../api-gateway.toml",
            "rust/api-gateway/api-gateway.toml",
        ];

        for path in &candidates {
            if Path::new(path).exists() {
                return Self::from_file(path);
            }
        }

        Err(ConfigError::NotFound)
    }

    pub fn load_or_default() -> Self {
        Self::load().unwrap_or_default()
    }

    pub fn apply_to_env(&self) {
        unsafe {
            std::env::set_var("GLOBAL_JWKS_ENDPOINT", &self.auth.jwks_endpoint);
            std::env::set_var("JWKS_ENDPOINT", &self.auth.jwks_endpoint);
            std::env::set_var("REDIS_URL", &self.redis.url);
            std::env::set_var("SERVER_HOST", &self.server.host);
            std::env::set_var("SERVER_PORT", self.server.port.to_string());
            std::env::set_var(
                "MAX_CONTENT_LENGTH_BYTES",
                self.limits.max_content_length_bytes.to_string(),
            );
            std::env::set_var(
                "REQUEST_TIMEOUT_SECS",
                self.limits.request_timeout_secs.to_string(),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_valid_toml() {
        let toml_str = r#"
            [server]
            host = "127.0.0.1"
            port = 9000

            [auth]
            jwks_endpoint = "https://custom.internal/.well_known/jwks.json"

            [redis]
            url = "redis://redis-server:6379"

            [limits]
            max_content_length_bytes = 5242880
            request_timeout_secs = 15
            min_cooldown_secs = 30
            max_cooldown_secs = 120
        "#;

        let config: GatewayConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.server.host, "127.0.0.1");
        assert_eq!(config.server.port, 9000);
        assert_eq!(
            config.auth.jwks_endpoint,
            "https://custom.internal/.well_known/jwks.json"
        );
        assert_eq!(config.redis.url, "redis://redis-server:6379");
        assert_eq!(config.limits.max_content_length_bytes, 5242880);
        assert_eq!(config.limits.request_timeout_secs, 15);
        assert_eq!(config.limits.min_cooldown_secs, 30);
        assert_eq!(config.limits.max_cooldown_secs, 120);
    }

    #[test]
    fn test_parse_empty_toml_falls_back_to_defaults() {
        let config: GatewayConfig = toml::from_str("").unwrap();
        assert_eq!(config.server.host, DEFAULT_HOST);
        assert_eq!(config.server.port, DEFAULT_PORT);
        assert_eq!(config.auth.jwks_endpoint, DEFAULT_JWKS_ENDPOINT);
        assert_eq!(config.redis.url, DEFAULT_REDIS_URL);
        assert_eq!(config.limits.max_content_length_bytes, DEFAULT_MAX_CONTENT_LENGTH);
    }

    #[test]
    fn test_apply_to_env_populates_vars() {
        let config = GatewayConfig {
            server: ServerConfig {
                host: "10.0.0.1".into(),
                port: 7000,
            },
            auth: AuthConfig {
                jwks_endpoint: "https://auth.internal/.well_known/jwks.json".into(),
            },
            redis: RedisConfig {
                url: "redis://10.0.0.2:6379".into(),
            },
            limits: LimitsConfig::default(),
        };

        config.apply_to_env();

        assert_eq!(std::env::var("SERVER_HOST").unwrap(), "10.0.0.1");
        assert_eq!(std::env::var("SERVER_PORT").unwrap(), "7000");
        assert_eq!(std::env::var("REDIS_URL").unwrap(), "redis://10.0.0.2:6379");
        assert_eq!(
            std::env::var("GLOBAL_JWKS_ENDPOINT").unwrap(),
            "https://auth.internal/.well_known/jwks.json"
        );
    }
}
