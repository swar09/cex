use std::time::Duration;

use jsonwebtoken::jwk::{Jwk, JwkSet};
use redis::{
    AsyncCommands, Client, MSetOptions, SetExpiry,
    aio::{ConnectionManager, ConnectionManagerConfig},
};
use thiserror::Error;

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
    pub auth_conn: ConnectionManager,
    pub jwks_endpoint: Option<String>,
}

impl Cache {
    pub async fn new(auth_redis_addr: &str, jwks_endpoint: Option<String>) -> Result<Self, CacheError> {
        let config = ConnectionManagerConfig::new()
            .set_connection_timeout(Some(Duration::from_secs(3)))
            .set_response_timeout(Some(Duration::from_secs(2)));
        let auth_client = Client::open(auth_redis_addr)?;
        let auth_conn = ConnectionManager::new_with_config(auth_client, config.clone()).await?;

        let cache = Cache { auth_conn, jwks_endpoint };

        if cache.jwks_endpoint.is_some() {
            match cache.load_jwks().await {
                Ok(b) => {
                    if !b {
                        eprintln!("[cache] warning: startup JWKS load failed (will lazy-load on demand)");
                    }
                },
                Err(e) => {
                    eprintln!("[cache] error: startup JWKS load failed (will lazy-load on demand): {e}");
                },
            }
        }
        Ok(cache)
    }
    pub async fn load_jwks(&self) -> Result<bool, CacheError> {
        todo!()
    }
    pub async fn fetch_jwks_from_endpoint() -> Result<JwkSet, CacheError> {
        //
        todo!()
    }
    pub async fn batch_mset_jwks(&self, jwks: Vec<(&str, &Jwk)>, exp_sec: u64) -> Result<bool, CacheError> {
        let mut conn = self.auth_conn.clone();
        let mut items = Vec::new();
        for (kid, jwk) in jwks {
            let key = format!("jwks:{kid}");
            let json_str = serde_json::to_string(jwk)?;
            items.push((key, json_str));
        }
        let options = MSetOptions::default().with_expiration(SetExpiry::EX(exp_sec));
        let _: () = conn.mset_ex(&items, options).await?;
        return Ok(true);
    }
}
