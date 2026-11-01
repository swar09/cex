use jsonwebtoken::jwk::{Jwk, JwkSet};
use redis::{
    AsyncCommands, Client, SetOptions,
    aio::{ConnectionManager, ConnectionManagerConfig},
};
use regex::Regex;
use reqwest::Client as HttpClient;
use std::{
    collections::HashMap,
    error::Error,
    sync::{Arc, LazyLock, RwLock},
    time::{Duration, Instant},
};
use thiserror::Error;
use uuid::Uuid;
use validator::{Validate, ValidationErrors};

static JWKS_URL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^https://[^/#?]+/\.well_known/jwks\.json$").expect("Failed to compile JWKS URL regex")
});

const MIN_COOLDOWN_SECS: u64 = 60;
const MAX_COOLDOWN_SECS: u64 = 240;

/// Default JWKS endpoint fallback when not defined in environment variables.
pub const DEFAULT_JWKS_ENDPOINT: &str = "https://auth.internal/.well_known/jwks.json";

/// Errors occurring during cache operations.
#[derive(Debug, Error)]
pub enum CacheError {
    #[error("Redis error: {0}")]
    Redis(#[from] redis::RedisError),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Key not found: {0}")]
    NotFound(String),

    #[error("Fetch error: {0}")]
    FetchError(String),

    #[error("Parse integer error: {0}")]
    ParseInt(#[from] std::num::ParseIntError),

    #[error("Cooldown is active, cannot fetch JWKS")]
    CooldownActive,
}

/// Local in-memory JWKS cache and Redis token blacklist client.
#[derive(Clone)]
pub struct Cache {
    auth_conn: Option<ConnectionManager>,
    local_cache: Arc<RwLock<HashMap<String, Jwk>>>,
    jwks_client: JwksClient,
    cooldown_until: Arc<RwLock<Option<Instant>>>,
}

impl Cache {
    /// Constructs `Cache` connected to Redis for blacklist tracking with in-memory `RwLock` JWKS cache.
    pub async fn new(auth_redis_addr: &str, jwks_client: JwksClient) -> Result<Self, CacheError> {
        let config = ConnectionManagerConfig::new()
            .set_connection_timeout(Some(Duration::from_secs(3)))
            .set_response_timeout(Some(Duration::from_secs(2)));
        let auth_client = Client::open(auth_redis_addr)?;
        let auth_conn = ConnectionManager::new_with_config(auth_client, config).await?;
        let local_cache = Arc::new(RwLock::new(HashMap::new()));
        let cooldown_until = Arc::new(RwLock::new(None));

        let cache = Cache {
            auth_conn: Some(auth_conn),
            local_cache,
            jwks_client,
            cooldown_until,
        };

        if let Err(e) = cache.refill_jwks().await {
            eprintln!("[cache] warning: startup JWKS load failed (will lazy-load on demand): {e}");
        }

        Ok(cache)
    }

    /// Constructs in-memory only `Cache` without Redis connection.
    pub fn new_in_memory(jwks_client: JwksClient) -> Self {
        Cache {
            auth_conn: None,
            local_cache: Arc::new(RwLock::new(HashMap::new())),
            jwks_client,
            cooldown_until: Arc::new(RwLock::new(None)),
        }
    }

    /// Calculates random cooldown duration between 1 and 4 minutes (60-240 seconds).
    pub fn calculate_jitter_cooldown() -> Duration {
        let secs = fastrand::u64(MIN_COOLDOWN_SECS..=MAX_COOLDOWN_SECS);
        Duration::from_secs(secs)
    }

    /// Checks if request cooldown is currently active.
    pub fn is_cooldown_active(&self) -> bool {
        if let Ok(guard) = self.cooldown_until.read() {
            if let Some(until) = *guard {
                return Instant::now() < until;
            }
        }
        false
    }

    /// Sets cooldown timestamp to expire after specified `duration`.
    pub fn set_cooldown(&self, duration: Duration) {
        if let Ok(mut guard) = self.cooldown_until.write() {
            *guard = Some(Instant::now() + duration);
        }
    }

    /// Clears active cooldown immediately.
    pub fn clear_cooldown(&self) {
        if let Ok(mut guard) = self.cooldown_until.write() {
            *guard = None;
        }
    }

    /// Retrieves cloned `Jwk` from in-memory cache under shared read lock.
    pub fn get_local_jwk(&self, kid: &str) -> Option<Jwk> {
        let guard = self.local_cache.read().ok()?;
        guard.get(kid).cloned()
    }

    /// Empties existing keys in in-memory cache and populates with `jwks`.
    pub fn replace_local_jwks(&self, jwks: Vec<(String, Jwk)>) {
        if let Ok(mut guard) = self.local_cache.write() {
            guard.clear();
            for (kid, jwk) in jwks {
                guard.insert(kid, jwk);
            }
        }
    }

    /// Refills JWKS cache by fetching keys from endpoint and replacing in-memory map.
    pub async fn refill_jwks(&self) -> Result<bool, CacheError> {
        let set = self
            .jwks_client
            .fetch_jwks_from_endpoint()
            .await
            .map_err(|e| CacheError::FetchError(e.to_string()))?;

        let pairs: Vec<(String, Jwk)> = set
            .keys
            .into_iter()
            .filter_map(|jwk| jwk.common.key_id.clone().map(|kid| (kid, jwk)))
            .collect();

        self.replace_local_jwks(pairs);

        Ok(true)
    }

    /// Retrieves `Jwk` by key ID (`kid`), reading from local cache first.
    /// If missing and not under cooldown, lazy-fetches from JWKS endpoint and sets jittered cooldown.
    pub async fn get_or_fetch_jwk(&self, kid: &str) -> Result<Jwk, CacheError> {
        if let Some(jwk) = self.get_local_jwk(kid) {
            return Ok(jwk);
        }

        if self.is_cooldown_active() {
            return Err(CacheError::CooldownActive);
        }

        self.refill_jwks().await?;

        let cooldown = Self::calculate_jitter_cooldown();
        self.set_cooldown(cooldown);

        if let Some(jwk) = self.get_local_jwk(kid) {
            Ok(jwk)
        } else {
            Err(CacheError::NotFound(kid.to_string()))
        }
    }

    /// Marks token ID (`jti`) as revoked in Redis blacklist.
    pub async fn blacklist(&self, jti: Uuid, exp_sec: u64) -> Result<bool, CacheError> {
        let Some(mut conn) = self.auth_conn.clone() else {
            return Ok(true);
        };
        let options = SetOptions::default().with_expiration(redis::SetExpiry::EX(exp_sec));
        let key = format!("blacklist:{jti}");
        let value = format!("{jti}");
        let result: Option<String> = conn.set_options(key, value, options).await?;
        Ok(result.as_deref() == Some("OK"))
    }

    /// Checks if token ID (`jti`) is present in blacklist.
    pub async fn is_blacklist(&self, jti: Uuid) -> Result<bool, CacheError> {
        let Some(mut conn) = self.auth_conn.clone() else {
            return Ok(false);
        };
        let key = format!("blacklist:{jti}");
        let result: bool = conn.exists(key).await?;
        Ok(result)
    }
}

/// Client for fetching JWKS from validated HTTPS endpoint.
#[derive(Clone, Validate)]
pub struct JwksClient {
    pub client: HttpClient,
    #[validate(url, regex(path = *JWKS_URL_RE))]
    jwks_endpoint: String,
}

impl JwksClient {
    /// Validates and constructs new `JwksClient`.
    pub fn new(jwks_endpoint: String) -> Result<Self, JwksClientError> {
        let client = HttpClient::builder()
            .timeout(Duration::from_secs(5))
            .https_only(true)
            .build()
            .map_err(JwksClientError::ClientBuild)?;
        let this = Self { client, jwks_endpoint };
        this.validate()?;
        Ok(this)
    }

    /// Fetches JWKS keyset from configured remote endpoint.
    pub async fn fetch_jwks_from_endpoint(&self) -> Result<JwkSet, Box<dyn Error>> {
        let res = self.client.get(self.jwks_endpoint.clone()).send().await?;
        let jwkset: JwkSet = res.json().await?;
        Ok(jwkset)
    }
}

/// Resolves JWKS endpoint from `GLOBAL_JWKS_ENDPOINT` or `JWKS_ENDPOINT`,
/// falling back to [`DEFAULT_JWKS_ENDPOINT`] if neither is set.
pub fn get_jwks_endpoint_from_env() -> String {
    std::env::var("GLOBAL_JWKS_ENDPOINT")
        .or_else(|_| std::env::var("JWKS_ENDPOINT"))
        .unwrap_or_else(|_| DEFAULT_JWKS_ENDPOINT.to_string())
}

/// Errors occurring during `JwksClient` creation and fetching.
#[derive(Debug, thiserror::Error)]
pub enum JwksClientError {
    #[error("invalid jwks endpoint: {0}")]
    InvalidEndpoint(#[from] ValidationErrors),
    #[error("failed to build http client")]
    ClientBuild(#[source] reqwest::Error),
    #[error("failed to fetch jwks")]
    Fetch(#[from] reqwest::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_regex_compiles() {
        assert!(JWKS_URL_RE.is_match("https://auth.internal/.well_known/jwks.json"));
    }

    #[test]
    fn test_bad_jwks_endpoint_rejected_by_validate() {
        let endpoint = String::from("https://example.com/#?=/.well-known/jwks.json");
        let result = JwksClient::new(endpoint);
        assert!(matches!(result, Err(JwksClientError::InvalidEndpoint(_))));
    }

    #[test]
    fn test_valid_jwks_endpoint_accepted() {
        let endpoint = String::from("https://auth.internal/.well_known/jwks.json");
        let result = JwksClient::new(endpoint);
        assert!(result.is_ok());
    }

    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn test_env_fallback_to_default() {
        let _guard = ENV_LOCK.lock().unwrap();
        // Ensure no env vars set for test isolation
        unsafe {
            std::env::remove_var("GLOBAL_JWKS_ENDPOINT");
            std::env::remove_var("JWKS_ENDPOINT");
        }
        let endpoint = get_jwks_endpoint_from_env();
        assert_eq!(endpoint, DEFAULT_JWKS_ENDPOINT);
    }

    #[test]
    fn test_env_resolution_precedence() {
        let _guard = ENV_LOCK.lock().unwrap();
        unsafe {
            std::env::set_var("GLOBAL_JWKS_ENDPOINT", "https://global.internal/.well_known/jwks.json");
            std::env::set_var("JWKS_ENDPOINT", "https://local.internal/.well_known/jwks.json");
        }
        assert_eq!(get_jwks_endpoint_from_env(), "https://global.internal/.well_known/jwks.json");

        unsafe {
            std::env::remove_var("GLOBAL_JWKS_ENDPOINT");
            std::env::set_var("JWKS_ENDPOINT", "https://local.internal/.well_known/jwks.json");
        }
        assert_eq!(get_jwks_endpoint_from_env(), "https://local.internal/.well_known/jwks.json");

        unsafe {
            std::env::remove_var("JWKS_ENDPOINT");
        }
    }

    #[test]
    fn test_calculate_jitter_cooldown_range() {
        for _ in 0..100 {
            let duration = Cache::calculate_jitter_cooldown();
            assert!(duration >= Duration::from_secs(60));
            assert!(duration <= Duration::from_secs(240));
        }
    }

    #[test]
    fn test_cooldown_active_and_clear() {
        let client = JwksClient::new("https://auth.internal/.well_known/jwks.json".into()).unwrap();
        let cache = Cache::new_in_memory(client);

        assert!(!cache.is_cooldown_active());

        // Set cooldown for 10 seconds
        cache.set_cooldown(Duration::from_secs(10));
        assert!(cache.is_cooldown_active());

        // Clear cooldown
        cache.clear_cooldown();
        assert!(!cache.is_cooldown_active());
    }

    #[test]
    fn test_in_memory_cache_replace_and_get() {
        let client = JwksClient::new("https://auth.internal/.well_known/jwks.json".into()).unwrap();
        let cache = Cache::new_in_memory(client);

        let jwk_json = r#"{
            "kty": "RSA",
            "kid": "key-1",
            "n": "u1SU1LfVLPHCozMxH2Mo4lgOEePzMn0Wn6LbbhBhW-Xu8u0",
            "e": "AQAB"
        }"#;
        let jwk: Jwk = serde_json::from_str(jwk_json).unwrap();

        // Initially empty
        assert!(cache.get_local_jwk("key-1").is_none());

        // Populate
        cache.replace_local_jwks(vec![("key-1".to_string(), jwk.clone())]);
        assert_eq!(cache.get_local_jwk("key-1").unwrap().common.key_id.as_deref(), Some("key-1"));

        // Replacing empties existing keys
        let jwk2_json = r#"{
            "kty": "RSA",
            "kid": "key-2",
            "n": "u1SU1LfVLPHCozMxH2Mo4lgOEePzMn0Wn6LbbhBhW-Xu8u0",
            "e": "AQAB"
        }"#;
        let jwk2: Jwk = serde_json::from_str(jwk2_json).unwrap();
        cache.replace_local_jwks(vec![("key-2".to_string(), jwk2)]);

        // key-1 must be emptied, key-2 present
        assert!(cache.get_local_jwk("key-1").is_none());
        assert!(cache.get_local_jwk("key-2").is_some());
    }

    #[tokio::test]
    async fn test_cache_miss_with_active_cooldown_returns_error() {
        let client = JwksClient::new("https://auth.internal/.well_known/jwks.json".into()).unwrap();
        let cache = Cache::new_in_memory(client);

        cache.set_cooldown(Duration::from_secs(60));
        let result = cache.get_or_fetch_jwk("unknown-key").await;
        assert!(matches!(result, Err(CacheError::CooldownActive)));
    }
}
