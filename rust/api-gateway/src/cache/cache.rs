use jsonwebtoken::jwk::{Jwk, JwkSet};
use redis::{
    AsyncCommands, Client, MSetOptions, SetExpiry, SetOptions,
    aio::{ConnectionManager, ConnectionManagerConfig},
};
use regex::Regex;
use reqwest::Client as HttpClient;
use std::{
    collections::HashMap,
    error::Error,
    sync::{Arc, LazyLock, Mutex, atomic::AtomicBool},
    time::Duration,
};
use thiserror::Error;
use uuid::Uuid;
use validator::{Validate, ValidationErrors};

static JWKS_URL_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^https://[^/#?]+/\.well_known/jwks\.json$").expect("Failed to compile its "));
const JWKS_TTL: u64 = 300;

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
}

#[derive(Clone)]
pub struct Cache {
    auth_conn: ConnectionManager,
    local_cache: Arc<Mutex<HashMap<String, Jwk>>>,
    jwks_client: JwksClient,
    cool_down: Arc<AtomicBool>,
}

impl Cache {
    pub async fn new(auth_redis_addr: &str, jwks_client: JwksClient) -> Result<Self, CacheError> {
        let config = ConnectionManagerConfig::new()
            .set_connection_timeout(Some(Duration::from_secs(3)))
            .set_response_timeout(Some(Duration::from_secs(2)));
        let auth_client = Client::open(auth_redis_addr)?;
        let auth_conn = ConnectionManager::new_with_config(auth_client, config.clone()).await?;
        let local_cache = Arc::new(Mutex::new(HashMap::new()));
        let cool_down = Arc::new(AtomicBool::new(false));
        let cache = Cache {
            auth_conn,
            local_cache,
            jwks_client,
            cool_down,
        };

        match cache.load_jwks().await {
            Ok(b) => {
                if !b {
                    eprintln!("[cache] warning: startup JWKS load failed (will lazy-load on demand)");
                }
            },
            Err(e) => {
                eprintln!("[cache] error: startup JWKS load failed : {e}");
            },
        }

        Ok(cache)
    }
    pub async fn load_jwks(&self) -> Result<bool, CacheError> {
        let set = self
            .jwks_client
            .fetch_jwks_from_endpoint()
            .await
            .map_err(|e| CacheError::FetchError(e.to_string()))?;
        let kids_jwks: Vec<(&String, &Jwk)> = set
            .keys
            .iter()
            .filter_map(|jwk| jwk.common.key_id.as_ref().map(|kid| (kid, jwk)))
            .collect();
        return self.batch_mset_jwks(kids_jwks, JWKS_TTL).await;
    }

    pub async fn batch_mset_jwks(&self, kids_jwks: Vec<(&String, &Jwk)>, exp_sec: u64) -> Result<bool, CacheError> {
        let mut conn = self.auth_conn.clone();
        let mut items = Vec::new();
        for (kid, jwk) in kids_jwks {
            let key = format!("jwks:{kid}");
            let json_str = serde_json::to_string(jwk)?;
            items.push((key, json_str));
        }
        let options = MSetOptions::default().with_expiration(SetExpiry::EX(exp_sec));
        let _: () = conn.mset_ex(&items, options).await?;
        return Ok(true);
    }
    pub async fn blacklist(&self, jti: Uuid, exp_rsec: u64) -> Result<bool, CacheError> {
        let mut conn = self.auth_conn.clone();
        let options = SetOptions::default().with_expiration(redis::SetExpiry::EX(exp_rsec));
        let key = format!("blacklist:{jti}");
        let value = format!("{jti}");
        let result: Option<String> = conn.set_options(key, value, options).await?;

        Ok(result.as_deref() == Some("OK"))
    }
    pub async fn is_blacklist(&self, jti: Uuid) -> Result<bool, CacheError> {
        let mut conn = self.auth_conn.clone();
        let key = format!("blacklist:{jti}");
        let result: bool = conn.exists(key).await?;

        Ok(result)
    }
    pub async fn get_jwk(&self, kid: &str) -> Result<Option<Jwk>, CacheError> {
        let mut conn = self.auth_conn.clone();
        let key = format!("jwks:{kid}");
        let data: Option<String> = conn.get(key).await?;
        match data {
            Some(json_str) => {
                let jwk: Jwk = serde_json::from_str(&json_str)?;
                Ok(Some(jwk))
            },
            None => Ok(None),
        }
    }
    pub async fn get_or_fetch_jwk(&self, kid: &str) -> Result<Jwk, CacheError> {
        if let Some(cached) = self.get_jwk(kid).await? {
            return Ok(cached);
        }

        let _ = self.load_jwks().await?;
        if let Some(cached) = self.get_jwk(kid).await? {
            return Ok(cached);
        }
        return Err(CacheError::FetchError("error: key not found at refresh".into()));
    }
}

#[derive(Clone, Validate)]
pub struct JwksClient {
    pub client: HttpClient,
    #[validate(url, regex(path = * JWKS_URL_RE))]
    jwks_endpoint: String,
}
impl JwksClient {
    pub fn new(jwks_endpoint: String) -> Result<Self, JwksClientError> {
        let client = HttpClient::builder()
            .timeout(Duration::from_secs(5))
            .https_only(true)
            .build()
            .map_err(JwksClientError::ClientBuild)?;
        let this = Self { client, jwks_endpoint };
        this.validate()?;
        return Ok(this);
    }
    pub async fn fetch_jwks_from_endpoint(&self) -> Result<JwkSet, Box<dyn Error>> {
        let res = self.client.get(self.jwks_endpoint.clone()).send().await?;
        let jwkset: JwkSet = res.json().await?;
        return Ok(jwkset);
    }
}

#[derive(Debug, thiserror::Error)]
pub enum JwksClientError {
    #[error("invalid jwks endpoint: {0}")]
    InvalidEndpoint(#[from] ValidationErrors),
    #[error("failed to build http client")]
    ClientBuild(#[source] reqwest::Error),
    #[error("failed to fetch jwks")]
    Fetch(#[from] reqwest::Error),
}

mod test {
    use super::*;

    #[test]
    fn regex_compiles() {
        LazyLock::force(&JWKS_URL_RE);
    }
    #[test]
    fn bad_jwks_endpoint_rejected_by_validate() {
        let endpoint = String::from("https://exmaple.com/#?=/.well-known/jwks.json");
        let result = JwksClient::new(endpoint);
        assert!(matches!(result, Err(JwksClientError::InvalidEndpoint(_))))
    }
}
