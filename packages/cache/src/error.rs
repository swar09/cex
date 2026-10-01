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
}
