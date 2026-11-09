use std::{collections::HashMap, fs, path::Path};

use serde::{Deserialize, Serialize};
use thiserror::Error;

const DEFAULT_HOST: &str = "0.0.0.0";
const DEFAULT_PORT: u16 = 8080;
const DEFAULT_JWKS_ENDPOINT: &str = "https://auth.internal/.well_known/jwks.json";
const DEFAULT_REDIS_URL: &str = "redis://127.0.0.1:6379";
const DEFAULT_MAX_CONTENT_LENGTH: u64 = 10 * 1024 * 1024;
const DEFAULT_TIMEOUT_SECS: u64 = 30;
const DEFAULT_MIN_COOLDOWN_SECS: u64 = 60;
const DEFAULT_MAX_COOLDOWN_SECS: u64 = 240;
const DEFAULT_ORDERS_URL: &str = "http://order-service:8081";
const DEFAULT_ENGINE_URL: &str = "http://engine-service:8082";
const DEFAULT_LEDGER_URL: &str = "http://ledger-service:8083";
const DEFAULT_USER_URL: &str = "http://user-service:8084";
const DEFAULT_KYC_URL: &str = "http://kyc-service:8085";
const DEFAULT_AUTH_URL: &str = "http://auth-service:8086";
const DEFAULT_RATE_LIMIT_ENABLED: bool = false;
const DEFAULT_RATE_LIMIT_REDIS_URL: &str = "redis://127.0.0.1:16379";
const DEFAULT_RATE_LIMIT_KEY_PREFIX: &str = "rl";
const DEFAULT_RATE_LIMIT_LEASE_DURATION_SECS: u64 = 5;
const DEFAULT_RATE_LIMIT_BATCH_WINDOW_MICROS: u64 = 2000;
pub const DEFAULT_JWT_CACHE_ENABLED: bool = true;
pub const DEFAULT_JWT_CACHE_TTL_SECS: u64 = 15;
pub const DEFAULT_JWT_CACHE_MAX_ENTRIES_PER_SHARD: usize = 5000;
pub const DEFAULT_JWT_CACHE_SWEEP_INTERVAL_MS: u64 = 200;

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

#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Default)]
pub struct ApiGatewayConfig {
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub auth: AuthConfig,
    #[serde(default)]
    pub redis: RedisConfig,
    #[serde(default)]
    pub limits: LimitsConfig,
    #[serde(default)]
    pub services: ServicesConfig,
    #[serde(default)]
    pub rate_limiter: RateLimiterConfig,
    #[serde(default)]
    pub jwt_cache: JwtCacheConfig,
}

pub type GatewayConfig = ApiGatewayConfig;

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
    #[serde(default)]
    pub jwt_cache: Option<JwtCacheConfig>,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            jwks_endpoint: default_jwks_endpoint(),
            jwt_cache: None,
        }
    }
}

fn default_jwks_endpoint() -> String {
    DEFAULT_JWKS_ENDPOINT.to_string()
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct JwtCacheConfig {
    #[serde(default = "default_jwt_cache_enabled")]
    pub enabled: bool,
    #[serde(default = "default_jwt_cache_ttl_secs")]
    pub ttl_secs: u64,
    #[serde(default = "default_jwt_cache_max_entries_per_shard")]
    pub max_entries_per_shard: usize,
    #[serde(default = "default_jwt_cache_sweep_interval_ms")]
    pub sweep_interval_ms: u64,
}

impl Default for JwtCacheConfig {
    fn default() -> Self {
        Self {
            enabled: default_jwt_cache_enabled(),
            ttl_secs: default_jwt_cache_ttl_secs(),
            max_entries_per_shard: default_jwt_cache_max_entries_per_shard(),
            sweep_interval_ms: default_jwt_cache_sweep_interval_ms(),
        }
    }
}

fn default_jwt_cache_enabled() -> bool {
    DEFAULT_JWT_CACHE_ENABLED
}

fn default_jwt_cache_ttl_secs() -> u64 {
    std::env::var("JWT_CACHE_TTL_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_JWT_CACHE_TTL_SECS)
}

fn default_jwt_cache_max_entries_per_shard() -> usize {
    std::env::var("JWT_CACHE_MAX_ENTRIES_PER_SHARD")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_JWT_CACHE_MAX_ENTRIES_PER_SHARD)
}

fn default_jwt_cache_sweep_interval_ms() -> u64 {
    std::env::var("JWT_CACHE_SWEEP_INTERVAL_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_JWT_CACHE_SWEEP_INTERVAL_MS)
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct RedisConfig {
    #[serde(default = "default_redis_url")]
    pub url: String,
}

impl Default for RedisConfig {
    fn default() -> Self {
        Self { url: default_redis_url() }
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

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct ServicesConfig {
    #[serde(default = "default_orders_url")]
    pub orders: String,
    #[serde(default = "default_engine_url")]
    pub engine: String,
    #[serde(default = "default_ledger_url")]
    pub ledger: String,
    #[serde(default = "default_user_url")]
    pub user: String,
    #[serde(default = "default_kyc_url")]
    pub kyc: String,
    #[serde(default = "default_auth_url")]
    pub auth: String,
}

impl Default for ServicesConfig {
    fn default() -> Self {
        Self {
            orders: default_orders_url(),
            engine: default_engine_url(),
            ledger: default_ledger_url(),
            user: default_user_url(),
            kyc: default_kyc_url(),
            auth: default_auth_url(),
        }
    }
}

impl ServicesConfig {
    pub fn resolve_upstream(&self, path: &str) -> Option<&str> {
        if path.starts_with("/api/v1/orders") {
            Some(&self.orders)
        } else if path.starts_with("/api/v1/engine") {
            Some(&self.engine)
        } else if path.starts_with("/api/v1/ledger") {
            Some(&self.ledger)
        } else if path.starts_with("/api/v1/user") {
            Some(&self.user)
        } else if path.starts_with("/api/v1/kyc") {
            Some(&self.kyc)
        } else if path.starts_with("/api/v1/auth") {
            Some(&self.auth)
        } else {
            None
        }
    }
}

fn default_orders_url() -> String {
    std::env::var("ORDER_SERVICE_URL").unwrap_or_else(|_| DEFAULT_ORDERS_URL.to_string())
}

fn default_engine_url() -> String {
    std::env::var("ENGINE_SERVICE_URL").unwrap_or_else(|_| DEFAULT_ENGINE_URL.to_string())
}

fn default_ledger_url() -> String {
    std::env::var("LEDGER_SERVICE_URL").unwrap_or_else(|_| DEFAULT_LEDGER_URL.to_string())
}

fn default_user_url() -> String {
    std::env::var("USER_SERVICE_URL").unwrap_or_else(|_| DEFAULT_USER_URL.to_string())
}

fn default_kyc_url() -> String {
    std::env::var("KYC_SERVICE_URL").unwrap_or_else(|_| DEFAULT_KYC_URL.to_string())
}

fn default_auth_url() -> String {
    std::env::var("AUTH_SERVICE_URL").unwrap_or_else(|_| DEFAULT_AUTH_URL.to_string())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    Tier1,
    Tier2,
    Tier3,
    #[default]
    Tier4,
}

impl Tier {
    #[inline]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Tier1 => "tier1",
            Self::Tier2 => "tier2",
            Self::Tier3 => "tier3",
            Self::Tier4 => "tier4",
        }
    }
}

impl std::str::FromStr for Tier {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "tier1" => Ok(Self::Tier1),
            "tier2" => Ok(Self::Tier2),
            "tier3" => Ok(Self::Tier3),
            _ => Ok(Self::Tier4),
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct TierLimitConfig {
    pub initial_tokens: u64,
    pub fill_rate: u64,
    pub burst_interval_secs: u64,
    pub max_tokens: u64,
    pub lease_batch_size: u64,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct TiersConfig {
    #[serde(default = "default_tier1_config")]
    pub tier1: TierLimitConfig,
    #[serde(default = "default_tier2_config")]
    pub tier2: TierLimitConfig,
    #[serde(default = "default_tier3_config")]
    pub tier3: TierLimitConfig,
    #[serde(default = "default_tier4_config")]
    pub tier4: TierLimitConfig,
}

impl Default for TiersConfig {
    fn default() -> Self {
        Self {
            tier1: default_tier1_config(),
            tier2: default_tier2_config(),
            tier3: default_tier3_config(),
            tier4: default_tier4_config(),
        }
    }
}

impl TiersConfig {
    #[inline]
    pub fn get(&self, tier: Tier) -> &TierLimitConfig {
        match tier {
            Tier::Tier1 => &self.tier1,
            Tier::Tier2 => &self.tier2,
            Tier::Tier3 => &self.tier3,
            Tier::Tier4 => &self.tier4,
        }
    }
}

pub fn default_tier1_config() -> TierLimitConfig {
    TierLimitConfig {
        initial_tokens: 1000,
        fill_rate: 1000,
        burst_interval_secs: 60,
        max_tokens: 5000,
        lease_batch_size: 500,
    }
}

pub fn default_tier2_config() -> TierLimitConfig {
    TierLimitConfig {
        initial_tokens: 200,
        fill_rate: 200,
        burst_interval_secs: 60,
        max_tokens: 1000,
        lease_batch_size: 100,
    }
}

pub fn default_tier3_config() -> TierLimitConfig {
    TierLimitConfig {
        initial_tokens: 50,
        fill_rate: 50,
        burst_interval_secs: 60,
        max_tokens: 250,
        lease_batch_size: 50,
    }
}

pub fn default_tier4_config() -> TierLimitConfig {
    TierLimitConfig {
        initial_tokens: 10,
        fill_rate: 10,
        burst_interval_secs: 60,
        max_tokens: 50,
        lease_batch_size: 10,
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct RateLimiterConfig {
    #[serde(default = "default_rate_limit_enabled")]
    pub enabled: bool,
    #[serde(default = "default_rate_limit_redis_url")]
    pub redis_url: String,
    #[serde(default = "default_rate_limit_key_prefix")]
    pub key_prefix: String,
    #[serde(default = "default_rate_limit_lease_duration_secs")]
    pub lease_duration_secs: u64,
    #[serde(default = "default_rate_limit_batch_window_micros")]
    pub batch_window_micros: u64,
    #[serde(default)]
    pub default_tier: Tier,
    #[serde(default)]
    pub tiers: TiersConfig,
    #[serde(default = "default_role_mapping")]
    pub role_mapping: HashMap<String, Tier>,
}

impl Default for RateLimiterConfig {
    fn default() -> Self {
        Self {
            enabled: default_rate_limit_enabled(),
            redis_url: default_rate_limit_redis_url(),
            key_prefix: default_rate_limit_key_prefix(),
            lease_duration_secs: default_rate_limit_lease_duration_secs(),
            batch_window_micros: default_rate_limit_batch_window_micros(),
            default_tier: Tier::Tier4,
            tiers: TiersConfig::default(),
            role_mapping: default_role_mapping(),
        }
    }
}

fn default_rate_limit_enabled() -> bool {
    DEFAULT_RATE_LIMIT_ENABLED
}

fn default_rate_limit_redis_url() -> String {
    std::env::var("RATE_LIMIT_REDIS_URL").unwrap_or_else(|_| DEFAULT_RATE_LIMIT_REDIS_URL.to_string())
}

fn default_rate_limit_key_prefix() -> String {
    DEFAULT_RATE_LIMIT_KEY_PREFIX.to_string()
}

fn default_rate_limit_lease_duration_secs() -> u64 {
    DEFAULT_RATE_LIMIT_LEASE_DURATION_SECS
}

fn default_rate_limit_batch_window_micros() -> u64 {
    DEFAULT_RATE_LIMIT_BATCH_WINDOW_MICROS
}

fn default_role_mapping() -> HashMap<String, Tier> {
    let mut mapping = HashMap::new();
    mapping.insert("admin".to_string(), Tier::Tier1);
    mapping.insert("partner".to_string(), Tier::Tier1);
    mapping.insert("bot".to_string(), Tier::Tier2);
    mapping.insert("client".to_string(), Tier::Tier3);
    mapping.insert("default".to_string(), Tier::Tier4);
    mapping
}

impl GatewayConfig {
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, ConfigError> {
        let path_ref = path.as_ref();
        let path_str = path_ref.to_string_lossy().to_string();
        let content = fs::read_to_string(path_ref).map_err(|e| ConfigError::Io {
            path: path_str.clone(),
            source: e,
        })?;
        let mut config: GatewayConfig = toml::from_str(&content).map_err(|e| ConfigError::Parse { path: path_str, source: e })?;
        if let Some(sub_jwt_cache) = config.auth.jwt_cache.take() {
            config.jwt_cache = sub_jwt_cache;
        }
        Ok(config)
    }

    pub fn load() -> Result<Self, ConfigError> {
        if let Ok(custom_path) = std::env::var("API_GATEWAY_CONFIG") {
            return Self::from_file(custom_path);
        }

        let candidates = ["api-gateway.toml", "../../api-gateway.toml", "rust/api-gateway/api-gateway.toml"];

        for path in &candidates {
            if Path::new(path).exists() {
                return Self::from_file(path);
            }
        }

        Err(ConfigError::NotFound)
    }

    pub fn apply_env_overrides(&mut self) {
        if let Ok(host) = std::env::var("SERVER_HOST") {
            self.server.host = host;
        }
        if let Ok(port) = std::env::var("SERVER_PORT").and_then(|p| p.parse().map_err(|_| std::env::VarError::NotPresent)) {
            self.server.port = port;
        }
        if let Ok(jwks) = std::env::var("GLOBAL_JWKS_ENDPOINT") {
            self.auth.jwks_endpoint = jwks;
        } else if let Ok(jwks) = std::env::var("JWKS_ENDPOINT") {
            self.auth.jwks_endpoint = jwks;
        }
        if let Ok(redis) = std::env::var("REDIS_URL") {
            self.redis.url = redis;
        }
        if let Ok(orders) = std::env::var("ORDER_SERVICE_URL") {
            self.services.orders = orders;
        }
        if let Ok(engine) = std::env::var("ENGINE_SERVICE_URL") {
            self.services.engine = engine;
        }
        if let Ok(ledger) = std::env::var("LEDGER_SERVICE_URL") {
            self.services.ledger = ledger;
        }
        if let Ok(user) = std::env::var("USER_SERVICE_URL") {
            self.services.user = user;
        }
        if let Ok(kyc) = std::env::var("KYC_SERVICE_URL") {
            self.services.kyc = kyc;
        }
        if let Ok(auth) = std::env::var("AUTH_SERVICE_URL") {
            self.services.auth = auth;
        }
        if let Ok(timeout) = std::env::var("REQUEST_TIMEOUT_SECS").and_then(|t| t.parse().map_err(|_| std::env::VarError::NotPresent)) {
            self.limits.request_timeout_secs = timeout;
        }
        if let Ok(limit) = std::env::var("MAX_CONTENT_LENGTH_BYTES").and_then(|l| l.parse().map_err(|_| std::env::VarError::NotPresent)) {
            self.limits.max_content_length_bytes = limit;
        }
        if let Ok(enabled) = std::env::var("RATE_LIMIT_ENABLED").and_then(|v| v.parse().map_err(|_| std::env::VarError::NotPresent)) {
            self.rate_limiter.enabled = enabled;
        }
        if let Ok(redis_url) = std::env::var("RATE_LIMIT_REDIS_URL") {
            self.rate_limiter.redis_url = redis_url;
        }
        if let Ok(batch_window) = std::env::var("RATE_LIMIT_BATCH_WINDOW_MICROS").and_then(|v| v.parse().map_err(|_| std::env::VarError::NotPresent))
        {
            self.rate_limiter.batch_window_micros = batch_window;
        }
        if let Ok(lease_duration) =
            std::env::var("RATE_LIMIT_LEASE_DURATION_SECS").and_then(|v| v.parse().map_err(|_| std::env::VarError::NotPresent))
        {
            self.rate_limiter.lease_duration_secs = lease_duration;
        }
        if let Ok(enabled) = std::env::var("JWT_CACHE_ENABLED").and_then(|v| v.parse().map_err(|_| std::env::VarError::NotPresent)) {
            self.jwt_cache.enabled = enabled;
        }
        if let Ok(ttl) = std::env::var("JWT_CACHE_TTL_SECS").and_then(|v| v.parse().map_err(|_| std::env::VarError::NotPresent)) {
            self.jwt_cache.ttl_secs = ttl;
        }
        if let Ok(max_entries) = std::env::var("JWT_CACHE_MAX_ENTRIES_PER_SHARD").and_then(|v| v.parse().map_err(|_| std::env::VarError::NotPresent))
        {
            self.jwt_cache.max_entries_per_shard = max_entries;
        }
        if let Ok(sweep_ms) = std::env::var("JWT_CACHE_SWEEP_INTERVAL_MS").and_then(|v| v.parse().map_err(|_| std::env::VarError::NotPresent)) {
            self.jwt_cache.sweep_interval_ms = sweep_ms;
        }
    }

    pub fn load_or_default() -> Self {
        let mut config = Self::load().unwrap_or_default();
        config.apply_env_overrides();
        config
    }

    pub fn apply_to_env(&self) {
        unsafe {
            std::env::set_var("GLOBAL_JWKS_ENDPOINT", &self.auth.jwks_endpoint);
            std::env::set_var("JWKS_ENDPOINT", &self.auth.jwks_endpoint);
            std::env::set_var("REDIS_URL", &self.redis.url);
            std::env::set_var("SERVER_HOST", &self.server.host);
            std::env::set_var("SERVER_PORT", self.server.port.to_string());
            std::env::set_var("MAX_CONTENT_LENGTH_BYTES", self.limits.max_content_length_bytes.to_string());
            std::env::set_var("REQUEST_TIMEOUT_SECS", self.limits.request_timeout_secs.to_string());
            std::env::set_var("ORDER_SERVICE_URL", &self.services.orders);
            std::env::set_var("ENGINE_SERVICE_URL", &self.services.engine);
            std::env::set_var("LEDGER_SERVICE_URL", &self.services.ledger);
            std::env::set_var("USER_SERVICE_URL", &self.services.user);
            std::env::set_var("KYC_SERVICE_URL", &self.services.kyc);
            std::env::set_var("AUTH_SERVICE_URL", &self.services.auth);
            std::env::set_var("RATE_LIMIT_ENABLED", self.rate_limiter.enabled.to_string());
            std::env::set_var("RATE_LIMIT_REDIS_URL", &self.rate_limiter.redis_url);
            std::env::set_var("RATE_LIMIT_LEASE_DURATION_SECS", self.rate_limiter.lease_duration_secs.to_string());
            std::env::set_var("RATE_LIMIT_BATCH_WINDOW_MICROS", self.rate_limiter.batch_window_micros.to_string());
            std::env::set_var("JWT_CACHE_ENABLED", self.jwt_cache.enabled.to_string());
            std::env::set_var("JWT_CACHE_TTL_SECS", self.jwt_cache.ttl_secs.to_string());
            std::env::set_var("JWT_CACHE_MAX_ENTRIES_PER_SHARD", self.jwt_cache.max_entries_per_shard.to_string());
            std::env::set_var("JWT_CACHE_SWEEP_INTERVAL_MS", self.jwt_cache.sweep_interval_ms.to_string());
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

            [services]
            orders = "http://order-test:8081"
            engine = "http://engine-test:8082"
            ledger = "http://ledger-test:8083"
            user = "http://user-test:8084"
            kyc = "http://kyc-test:8085"
            auth = "http://auth-test:8086"

            [rate_limiter]
            enabled = true
            batch_window_micros = 250
            lease_duration_secs = 10
        "#;

        let config: GatewayConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.server.host, "127.0.0.1");
        assert_eq!(config.server.port, 9000);
        assert_eq!(config.auth.jwks_endpoint, "https://custom.internal/.well_known/jwks.json");
        assert_eq!(config.redis.url, "redis://redis-server:6379");
        assert_eq!(config.limits.max_content_length_bytes, 5242880);
        assert_eq!(config.limits.request_timeout_secs, 15);
        assert_eq!(config.limits.min_cooldown_secs, 30);
        assert_eq!(config.limits.max_cooldown_secs, 120);
        assert_eq!(config.services.orders, "http://order-test:8081");
        assert_eq!(config.services.engine, "http://engine-test:8082");
        assert_eq!(config.services.ledger, "http://ledger-test:8083");
        assert_eq!(config.services.user, "http://user-test:8084");
        assert_eq!(config.services.kyc, "http://kyc-test:8085");
        assert_eq!(config.services.auth, "http://auth-test:8086");
        assert!(config.rate_limiter.enabled);
        assert_eq!(config.rate_limiter.batch_window_micros, 250);
        assert_eq!(config.rate_limiter.lease_duration_secs, 10);
    }

    #[test]
    fn test_parse_empty_toml_falls_back_to_defaults() {
        let config: GatewayConfig = toml::from_str("").unwrap();
        assert_eq!(config.server.host, DEFAULT_HOST);
        assert_eq!(config.server.port, DEFAULT_PORT);
        assert_eq!(config.auth.jwks_endpoint, DEFAULT_JWKS_ENDPOINT);
        assert_eq!(config.redis.url, DEFAULT_REDIS_URL);
        assert_eq!(config.limits.max_content_length_bytes, DEFAULT_MAX_CONTENT_LENGTH);
        assert_eq!(config.services.orders, DEFAULT_ORDERS_URL);
        assert_eq!(config.services.engine, DEFAULT_ENGINE_URL);
        assert_eq!(config.services.ledger, DEFAULT_LEDGER_URL);
        assert_eq!(config.services.user, DEFAULT_USER_URL);
        assert_eq!(config.services.kyc, DEFAULT_KYC_URL);
        assert_eq!(config.services.auth, DEFAULT_AUTH_URL);
        assert_eq!(config.jwt_cache.enabled, DEFAULT_JWT_CACHE_ENABLED);
        assert_eq!(config.jwt_cache.ttl_secs, DEFAULT_JWT_CACHE_TTL_SECS);
        assert_eq!(config.jwt_cache.max_entries_per_shard, DEFAULT_JWT_CACHE_MAX_ENTRIES_PER_SHARD);
        assert_eq!(config.jwt_cache.sweep_interval_ms, DEFAULT_JWT_CACHE_SWEEP_INTERVAL_MS);
    }

    #[test]
    fn test_services_resolve_upstream() {
        let services = ServicesConfig::default();
        assert_eq!(services.resolve_upstream("/api/v1/orders/create"), Some(DEFAULT_ORDERS_URL));
        assert_eq!(services.resolve_upstream("/api/v1/engine/match"), Some(DEFAULT_ENGINE_URL));
        assert_eq!(services.resolve_upstream("/api/v1/ledger/balance"), Some(DEFAULT_LEDGER_URL));
        assert_eq!(services.resolve_upstream("/api/v1/user/profile"), Some(DEFAULT_USER_URL));
        assert_eq!(services.resolve_upstream("/api/v1/kyc/verify"), Some(DEFAULT_KYC_URL));
        assert_eq!(services.resolve_upstream("/api/v1/auth/login"), Some(DEFAULT_AUTH_URL));
        assert_eq!(services.resolve_upstream("/unknown/path"), None);
    }

    #[test]
    fn test_apply_to_env_populates_vars() {
        let _guard = crate::TEST_ENV_LOCK.lock();
        let config = GatewayConfig {
            server: ServerConfig {
                host: "10.0.0.1".into(),
                port: 7000,
            },
            auth: AuthConfig {
                jwks_endpoint: "https://auth.internal/.well_known/jwks.json".into(),
                jwt_cache: None,
            },
            redis: RedisConfig {
                url: "redis://10.0.0.2:6379".into(),
            },
            limits: LimitsConfig::default(),
            services: ServicesConfig::default(),
            rate_limiter: RateLimiterConfig::default(),
            jwt_cache: JwtCacheConfig::default(),
        };

        config.apply_to_env();

        assert_eq!(std::env::var("SERVER_HOST").unwrap(), "10.0.0.1");
        assert_eq!(std::env::var("SERVER_PORT").unwrap(), "7000");
        assert_eq!(std::env::var("REDIS_URL").unwrap(), "redis://10.0.0.2:6379");
        assert_eq!(
            std::env::var("GLOBAL_JWKS_ENDPOINT").unwrap(),
            "https://auth.internal/.well_known/jwks.json"
        );
        assert_eq!(std::env::var("ORDER_SERVICE_URL").unwrap(), DEFAULT_ORDERS_URL);
        assert_eq!(std::env::var("RATE_LIMIT_BATCH_WINDOW_MICROS").unwrap(), "2000");
        assert_eq!(std::env::var("RATE_LIMIT_LEASE_DURATION_SECS").unwrap(), "5");
        assert_eq!(std::env::var("JWT_CACHE_ENABLED").unwrap(), "true");
        assert_eq!(std::env::var("JWT_CACHE_TTL_SECS").unwrap(), "15");
        assert_eq!(std::env::var("JWT_CACHE_MAX_ENTRIES_PER_SHARD").unwrap(), "5000");
        assert_eq!(std::env::var("JWT_CACHE_SWEEP_INTERVAL_MS").unwrap(), "200");
    }

    #[test]
    fn test_parse_jwt_cache_config() {
        let toml_str = r#"
            [jwt_cache]
            enabled = false
            ttl_secs = 60
            max_entries_per_shard = 10000
            sweep_interval_ms = 500
        "#;
        let config: GatewayConfig = toml::from_str(toml_str).unwrap();
        assert!(!config.jwt_cache.enabled);
        assert_eq!(config.jwt_cache.ttl_secs, 60);
        assert_eq!(config.jwt_cache.max_entries_per_shard, 10000);
        assert_eq!(config.jwt_cache.sweep_interval_ms, 500);
    }

    #[test]
    fn test_parse_auth_jwt_cache_nested() {
        let toml_str = r#"
            [auth.jwt_cache]
            enabled = true
            ttl_secs = 45
            max_entries_per_shard = 2000
            sweep_interval_ms = 100
        "#;
        let dir = std::env::temp_dir();
        let path = dir.join(format!("test_config_{}.toml", fastrand::u64(..)));
        std::fs::write(&path, toml_str).unwrap();
        let config = GatewayConfig::from_file(&path).unwrap();
        std::fs::remove_file(&path).ok();

        assert!(config.jwt_cache.enabled);
        assert_eq!(config.jwt_cache.ttl_secs, 45);
        assert_eq!(config.jwt_cache.max_entries_per_shard, 2000);
        assert_eq!(config.jwt_cache.sweep_interval_ms, 100);
    }

    #[test]
    fn test_jwt_cache_env_overrides() {
        let _guard = crate::TEST_ENV_LOCK.lock();
        unsafe {
            std::env::set_var("JWT_CACHE_ENABLED", "false");
            std::env::set_var("JWT_CACHE_TTL_SECS", "30");
            std::env::set_var("JWT_CACHE_MAX_ENTRIES_PER_SHARD", "8000");
            std::env::set_var("JWT_CACHE_SWEEP_INTERVAL_MS", "400");
        }
        let mut config = GatewayConfig::default();
        config.apply_env_overrides();
        assert!(!config.jwt_cache.enabled);
        assert_eq!(config.jwt_cache.ttl_secs, 30);
        assert_eq!(config.jwt_cache.max_entries_per_shard, 8000);
        assert_eq!(config.jwt_cache.sweep_interval_ms, 400);

        unsafe {
            std::env::remove_var("JWT_CACHE_ENABLED");
            std::env::remove_var("JWT_CACHE_TTL_SECS");
            std::env::remove_var("JWT_CACHE_MAX_ENTRIES_PER_SHARD");
            std::env::remove_var("JWT_CACHE_SWEEP_INTERVAL_MS");
        }
    }
}
