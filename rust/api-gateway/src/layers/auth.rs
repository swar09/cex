use std::{
    pin::Pin,
    sync::Arc,
    time::{Duration, Instant},
};

use axum::{
    extract::Request,
    http::{HeaderName, HeaderValue, StatusCode, header::AUTHORIZATION},
    response::{IntoResponse, Response},
};
use fxhash::{FxHashMap, hash64};
use jsonwebtoken::{DecodingKey, Validation, decode, decode_header};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tower::Service;
use uuid::Uuid;

use crate::cache::cache::Cache;

/// Claims contained within validated JWT.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Claims {
    /// Token issuer.
    pub iss: String,
    /// Token subject (user UUID).
    pub sub: Uuid,
    /// Issued at timestamp (epoch seconds).
    pub iat: u64,
    /// Not before timestamp (epoch seconds).
    pub nbf: u64,
    /// Expiration timestamp (epoch seconds).
    pub exp: u64,
    /// Intended audience.
    pub aud: Audience,
    /// User role within platform.
    pub role: UserRole,
    /// Unique JWT ID for revocation and blacklisting.
    pub jti: Uuid,
}

/// Permitted audiences for token consumption.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Audience {
    Order,
    Engine,
    Ledger,
    User,
    Kyc,
    Auth,
}

/// User roles authorized to interact with gateway.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum UserRole {
    Admin,
    Client,
    Partner,
    Bot,
}

/// Validates whether Key ID (`kid`) is 32-character lowercase hex thumbprint.
pub fn is_valid_kid(kid: &str) -> bool {
    kid.len() == 32 && kid.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

pub const NUM_JWT_CACHE_SHARDS: usize = 64;
pub const SHARD_MASK: usize = NUM_JWT_CACHE_SHARDS - 1;
pub const DEFAULT_MAX_ENTRIES_PER_SHARD: usize = crate::config::DEFAULT_JWT_CACHE_MAX_ENTRIES_PER_SHARD;
pub const DEFAULT_JWT_CACHE_TTL: Duration = Duration::from_secs(crate::config::DEFAULT_JWT_CACHE_TTL_SECS);
pub const DEFAULT_JWT_CACHE_SWEEP_INTERVAL: Duration = Duration::from_millis(crate::config::DEFAULT_JWT_CACHE_SWEEP_INTERVAL_MS);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CachedToken {
    pub claims: Claims,
    pub expires_at: Instant,
}

pub struct CacheShard {
    pub entries: RwLock<FxHashMap<u64, CachedToken>>,
}

#[derive(Clone)]
pub struct LocalJwtCache {
    pub shards: Arc<[CacheShard; NUM_JWT_CACHE_SHARDS]>,
    pub default_ttl: Duration,
    pub max_entries_per_shard: usize,
    pub sweep_interval: Duration,
    pub enabled: bool,
}

impl LocalJwtCache {
    pub fn new(default_ttl: Duration, max_entries_per_shard: usize) -> Arc<Self> {
        Self::with_options(default_ttl, max_entries_per_shard, DEFAULT_JWT_CACHE_SWEEP_INTERVAL, true)
    }

    pub fn with_options(default_ttl: Duration, max_entries_per_shard: usize, sweep_interval: Duration, enabled: bool) -> Arc<Self> {
        let shards: Vec<CacheShard> = (0..NUM_JWT_CACHE_SHARDS)
            .map(|_| CacheShard {
                entries: RwLock::new(FxHashMap::default()),
            })
            .collect();

        let boxed_shards: Box<[CacheShard; NUM_JWT_CACHE_SHARDS]> = shards
            .into_boxed_slice()
            .try_into()
            .unwrap_or_else(|_| panic!("failed to initialize shards"));

        let cache = Arc::new(Self {
            shards: Arc::from(boxed_shards),
            default_ttl,
            max_entries_per_shard,
            sweep_interval,
            enabled,
        });

        if enabled {
            cache.spawn_sweeper();
        }
        cache
    }

    pub fn from_config(config: &crate::config::JwtCacheConfig) -> Arc<Self> {
        Self::with_options(
            Duration::from_secs(config.ttl_secs),
            config.max_entries_per_shard,
            Duration::from_millis(config.sweep_interval_ms),
            config.enabled,
        )
    }

    #[inline(always)]
    fn shard(&self, hash: u64) -> &CacheShard {
        let idx = (hash as usize) & SHARD_MASK;
        &self.shards[idx]
    }

    pub fn get(&self, token_bytes: &[u8], now: Instant) -> Option<Claims> {
        if !self.enabled {
            return None;
        }
        let hash = hash64(token_bytes);
        let shard = self.shard(hash);
        let guard = shard.entries.read();
        if let Some(entry) = guard.get(&hash)
            && now < entry.expires_at
        {
            return Some(entry.claims.clone());
        }
        None
    }

    pub fn insert(&self, token_bytes: &[u8], claims: Claims) {
        if !self.enabled {
            return;
        }
        let hash = hash64(token_bytes);
        let shard = self.shard(hash);
        let now = Instant::now();
        let expires_at = now + self.default_ttl;

        let mut guard = shard.entries.write();
        if guard.len() >= self.max_entries_per_shard {
            guard.retain(|_, entry| now < entry.expires_at);
        }
        guard.insert(hash, CachedToken { claims, expires_at });
    }

    pub fn evict(&self, token_bytes: &[u8]) {
        let hash = hash64(token_bytes);
        let shard = self.shard(hash);
        let mut guard = shard.entries.write();
        guard.remove(&hash);
    }

    pub fn clear(&self) {
        for shard in self.shards.iter() {
            shard.entries.write().clear();
        }
    }

    pub fn len(&self) -> usize {
        self.shards.iter().map(|s| s.entries.read().len()).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn spawn_sweeper(self: &Arc<Self>) {
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            let cache = Arc::clone(self);
            let max_capacity = self.max_entries_per_shard;
            let sweep_interval = self.sweep_interval;
            handle.spawn(async move {
                let mut scratch_keys: Vec<u64> = Vec::with_capacity(max_capacity);
                let mut interval = tokio::time::interval(sweep_interval);
                let mut current_shard: usize = 0;

                loop {
                    interval.tick().await;
                    let now = Instant::now();
                    let shard = &cache.shards[current_shard];

                    scratch_keys.clear();

                    {
                        let guard = shard.entries.read();
                        for (hash, entry) in guard.iter() {
                            if now >= entry.expires_at {
                                scratch_keys.push(*hash);
                            }
                        }
                    }

                    if !scratch_keys.is_empty() {
                        let mut guard = shard.entries.write();
                        for hash in &scratch_keys {
                            if let Some(entry) = guard.get(hash)
                                && now >= entry.expires_at
                            {
                                guard.remove(hash);
                            }
                        }
                    }

                    current_shard = (current_shard + 1) & SHARD_MASK;
                }
            });
        }
    }
}

impl Default for LocalJwtCache {
    fn default() -> Self {
        let config = crate::config::JwtCacheConfig::default();
        let shards: Vec<CacheShard> = (0..NUM_JWT_CACHE_SHARDS)
            .map(|_| CacheShard {
                entries: RwLock::new(FxHashMap::default()),
            })
            .collect();

        let boxed_shards: Box<[CacheShard; NUM_JWT_CACHE_SHARDS]> = shards
            .into_boxed_slice()
            .try_into()
            .unwrap_or_else(|_| panic!("failed to initialize shards"));

        Self {
            shards: Arc::from(boxed_shards),
            default_ttl: Duration::from_secs(config.ttl_secs),
            max_entries_per_shard: config.max_entries_per_shard,
            sweep_interval: Duration::from_millis(config.sweep_interval_ms),
            enabled: config.enabled,
        }
    }
}

/// Shared state for authentication layer.
#[derive(Clone)]
pub struct AuthState {
    /// In-memory JWKS cache and blacklist client.
    pub cache: Cache,
    pub jwt_cache: Arc<LocalJwtCache>,
}

impl AuthState {
    /// Constructs new `AuthState` instance.
    pub fn new(cache: Cache) -> Self {
        let config = crate::config::JwtCacheConfig::default();
        Self {
            cache,
            jwt_cache: LocalJwtCache::from_config(&config),
        }
    }

    pub fn from_config(cache: Cache, config: &crate::config::JwtCacheConfig) -> Self {
        Self {
            cache,
            jwt_cache: LocalJwtCache::from_config(config),
        }
    }

    pub fn with_jwt_cache(cache: Cache, jwt_cache: Arc<LocalJwtCache>) -> Self {
        Self { cache, jwt_cache }
    }
}

/// Tower Layer wrapping inner service with JWT edge authentication.
#[derive(Clone)]
pub struct AuthLayer {
    state: Arc<AuthState>,
}

impl AuthLayer {
    /// Constructs new `AuthLayer` with provided state.
    pub fn new(state: Arc<AuthState>) -> Self {
        Self { state }
    }
}

impl<S> tower::Layer<S> for AuthLayer {
    type Service = AuthService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        AuthService {
            inner,
            state: self.state.clone(),
        }
    }
}

/// Tower Service intercepting incoming HTTP requests to validate JWTs.
#[derive(Clone)]
pub struct AuthService<S> {
    inner: S,
    state: Arc<AuthState>,
}

pub type BoxFut<E> = Pin<Box<dyn std::future::Future<Output = Result<Response, E>> + Send>>;

/// Immediately rejects request with HTTP status code.
fn reject<E: Send + 'static>(status: StatusCode) -> BoxFut<E> {
    Box::pin(async move { Ok(status.into_response()) })
}

impl<S> Service<Request> for AuthService<S>
where
    S: Service<Request, Response = Response> + Clone + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Send + 'static,
{
    type Response = Response;
    type Error = S::Error;
    type Future = BoxFut<Self::Error>;

    fn poll_ready(&mut self, cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut req: Request) -> Self::Future {
        let token = match req
            .headers()
            .get(AUTHORIZATION)
            .and_then(|h| h.to_str().ok())
            .and_then(|t| t.strip_prefix("Bearer "))
        {
            Some(t) if !t.trim().is_empty() => t.trim().to_owned(),
            _ => return reject(StatusCode::UNAUTHORIZED),
        };

        let jwt_header = match decode_header(&token) {
            Ok(header) => header,
            Err(_) => return reject(StatusCode::UNAUTHORIZED),
        };

        let kid = match jwt_header.kid {
            Some(ref k) if is_valid_kid(k) => k.clone(),
            _ => return reject(StatusCode::UNAUTHORIZED),
        };

        let path = req.uri().path().to_string();
        let state = self.state.clone();
        let mut inner = self.inner.clone();

        Box::pin(async move {
            let token_bytes = token.as_bytes();
            let now = Instant::now();

            let claims = if let Some(claims) = state.jwt_cache.get(token_bytes, now) {
                claims
            } else {
                let jwk = match state.cache.get_or_fetch_jwk(&kid).await {
                    Ok(j) => j,
                    Err(_) => return Ok(StatusCode::UNAUTHORIZED.into_response()),
                };

                let decoding_key = match DecodingKey::from_jwk(&jwk) {
                    Ok(k) => k,
                    Err(_) => return Ok(StatusCode::UNAUTHORIZED.into_response()),
                };

                let mut validation = Validation::new(jwt_header.alg);
                validation.validate_nbf = true;
                validation.validate_aud = false;

                let claims = match decode::<Claims>(&token, &decoding_key, &validation) {
                    Ok(token_data) => token_data.claims,
                    Err(_) => return Ok(StatusCode::UNAUTHORIZED.into_response()),
                };

                match state.cache.is_blacklist(claims.jti).await {
                    Ok(true) => return Ok(StatusCode::UNAUTHORIZED.into_response()),
                    Err(_) => return Ok(StatusCode::UNAUTHORIZED.into_response()),
                    Ok(false) => {},
                }

                state.jwt_cache.insert(token_bytes, claims.clone());
                claims
            };

            req.headers_mut().remove(AUTHORIZATION);

            if let Ok(val) = HeaderValue::from_str(&claims.sub.to_string()) {
                req.headers_mut().insert(HeaderName::from_static("x-user-id"), val);
            }
            let role_str = serde_json::to_string(&claims.role).unwrap_or_default().trim_matches('"').to_string();
            if let Ok(val) = HeaderValue::from_str(&role_str) {
                req.headers_mut().insert(HeaderName::from_static("x-user-role"), val);
            }
            if let Ok(val) = HeaderValue::from_str(&claims.jti.to_string()) {
                req.headers_mut().insert(HeaderName::from_static("x-token-id"), val);
            }

            req.extensions_mut().insert(claims);
            let response = inner.call(req).await;

            if let Ok(ref res) = response
                && res.status().is_success()
                && path == "/api/v1/auth/logout"
            {
                state.jwt_cache.evict(token_bytes);
            }

            response
        })
    }
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use axum::{body::Body, extract::Request, response::Response};
    use jsonwebtoken::{Algorithm, EncodingKey, Header, encode, jwk::Jwk};
    use tower::ServiceExt;

    use super::*;
    use crate::cache::cache::JwksClient;

    #[test]
    fn test_is_valid_kid_accepted() {
        assert!(is_valid_kid("8f4b2a19c0e34d58b7a6921ef5c2018a"));
        assert!(is_valid_kid("a1b2c3d4e5f60718293a4b5c6d7e8f90"));
        assert!(is_valid_kid("00000000000000000000000000000000"));
        assert!(is_valid_kid("ffffffffffffffffffffffffffffffff"));
    }

    #[test]
    fn test_is_valid_kid_rejected() {
        assert!(!is_valid_kid(""));
        assert!(!is_valid_kid("8f4b2a19c0e34d58b7a6921ef5c2018")); // 31 chars
        assert!(!is_valid_kid("8f4b2a19c0e34d58b7a6921ef5c2018a1")); // 33 chars
        assert!(!is_valid_kid("8F4B2A19C0E34D58B7A6921EF5C2018A")); // uppercase hex
        assert!(!is_valid_kid("8f4b2a19c0e34d58b7a6921ef5c2018g")); // non-hex 'g'
        assert!(!is_valid_kid("c458a234-9ff5-4e76-8051-5bf344ef37ea")); // UUID format with dashes
        assert!(!is_valid_kid("8f4b2a19c0e34d58 b7a6921ef5c2018")); // space
    }

    #[derive(Clone)]
    struct DummyInner;

    impl Service<Request> for DummyInner {
        type Response = Response;
        type Error = std::convert::Infallible;
        type Future = Pin<Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>>;

        fn poll_ready(&mut self, _cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<(), Self::Error>> {
            std::task::Poll::Ready(Ok(()))
        }

        fn call(&mut self, req: Request) -> Self::Future {
            let claims = req.extensions().get::<Claims>().cloned();
            Box::pin(async move {
                if let Some(c) = claims {
                    Ok(Response::builder().status(StatusCode::OK).body(Body::from(c.sub.to_string())).unwrap())
                } else {
                    Ok(Response::builder().status(StatusCode::NO_CONTENT).body(Body::empty()).unwrap())
                }
            })
        }
    }

    fn sample_jwk() -> (Jwk, EncodingKey) {
        // Octet symmetric key for test signatures (HS256)
        let raw_secret = b"my-ultra-secure-and-long-test-secret-32-bytes";
        let encoding_key = EncodingKey::from_secret(raw_secret);

        let k_b64 = "bXktdWx0cmEtc2VjdXJlLWFuZC1sb25nLXRlc3Qtc2VjcmV0LTMyLWJ5dGVz";
        let jwk_json = serde_json::json!({
            "kty": "oct",
            "kid": "a1b2c3d4e5f60718293a4b5c6d7e8f90",
            "alg": "HS256",
            "k": k_b64
        });
        let jwk: Jwk = serde_json::from_value(jwk_json).unwrap();
        (jwk, encoding_key)
    }

    fn current_timestamp() -> u64 {
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()
    }

    #[tokio::test]
    async fn test_auth_middleware_missing_header_rejected() {
        let client = JwksClient::new("https://auth.internal/.well_known/jwks.json".into()).unwrap();
        let cache = Cache::new_in_memory(client);
        let state = Arc::new(AuthState::new(cache));
        let mut service = AuthService { inner: DummyInner, state };

        let req = Request::builder().uri("/api/test").body(Body::empty()).unwrap();

        let response = service.ready().await.unwrap().call(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_auth_middleware_invalid_bearer_prefix_rejected() {
        let client = JwksClient::new("https://auth.internal/.well_known/jwks.json".into()).unwrap();
        let cache = Cache::new_in_memory(client);
        let state = Arc::new(AuthState::new(cache));
        let mut service = AuthService { inner: DummyInner, state };

        let req = Request::builder()
            .uri("/api/test")
            .header(AUTHORIZATION, "Basic username:password")
            .body(Body::empty())
            .unwrap();

        let response = service.ready().await.unwrap().call(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_auth_middleware_invalid_jwt_format_rejected() {
        let client = JwksClient::new("https://auth.internal/.well_known/jwks.json".into()).unwrap();
        let cache = Cache::new_in_memory(client);
        let state = Arc::new(AuthState::new(cache));
        let mut service = AuthService { inner: DummyInner, state };

        let req = Request::builder()
            .uri("/api/test")
            .header(AUTHORIZATION, "Bearer not.a.valid.jwt")
            .body(Body::empty())
            .unwrap();

        let response = service.ready().await.unwrap().call(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_auth_middleware_invalid_kid_rejected() {
        let client = JwksClient::new("https://auth.internal/.well_known/jwks.json".into()).unwrap();
        let cache = Cache::new_in_memory(client);
        let state = Arc::new(AuthState::new(cache));
        let mut service = AuthService { inner: DummyInner, state };

        let mut header = Header::new(Algorithm::HS256);
        header.kid = Some("../malformed/path".to_string());

        let now = current_timestamp();
        let claims = Claims {
            iss: "auth.internal".to_string(),
            sub: Uuid::new_v4(),
            iat: now,
            nbf: now - 10,
            exp: now + 3600,
            aud: Audience::Order,
            role: UserRole::Client,
            jti: Uuid::new_v4(),
        };

        let (_, encoding_key) = sample_jwk();
        let token = encode(&header, &claims, &encoding_key).unwrap();

        let req = Request::builder()
            .uri("/api/test")
            .header(AUTHORIZATION, format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap();

        let response = service.ready().await.unwrap().call(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_auth_middleware_cache_miss_during_cooldown_rejected() {
        let client = JwksClient::new("https://auth.internal/.well_known/jwks.json".into()).unwrap();
        let cache = Cache::new_in_memory(client);
        // Set cooldown active
        cache.set_cooldown(std::time::Duration::from_secs(60));

        let state = Arc::new(AuthState::new(cache));
        let mut service = AuthService { inner: DummyInner, state };

        let mut header = Header::new(Algorithm::HS256);
        header.kid = Some("00000000000000000000000000000000".to_string());

        let now = current_timestamp();
        let claims = Claims {
            iss: "auth.internal".to_string(),
            sub: Uuid::new_v4(),
            iat: now,
            nbf: now - 10,
            exp: now + 3600,
            aud: Audience::Order,
            role: UserRole::Client,
            jti: Uuid::new_v4(),
        };

        let (_, encoding_key) = sample_jwk();
        let token = encode(&header, &claims, &encoding_key).unwrap();

        let req = Request::builder()
            .uri("/api/test")
            .header(AUTHORIZATION, format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap();

        let response = service.ready().await.unwrap().call(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_auth_middleware_valid_token_authorized() {
        let (jwk, encoding_key) = sample_jwk();
        let client = JwksClient::new("https://auth.internal/.well_known/jwks.json".into()).unwrap();
        let cache = Cache::new_in_memory(client);

        // Prepopulate in-memory cache with the JWK
        cache.replace_local_jwks(vec![("a1b2c3d4e5f60718293a4b5c6d7e8f90".to_string(), jwk)]);

        let state = Arc::new(AuthState::new(cache));
        let mut service = AuthService { inner: DummyInner, state };

        let mut header = Header::new(Algorithm::HS256);
        header.kid = Some("a1b2c3d4e5f60718293a4b5c6d7e8f90".to_string());

        let user_id = Uuid::new_v4();
        let now = current_timestamp();
        let claims = Claims {
            iss: "auth.internal".to_string(),
            sub: user_id,
            iat: now,
            nbf: now - 10,
            exp: now + 3600,
            aud: Audience::Order,
            role: UserRole::Client,
            jti: Uuid::new_v4(),
        };

        let token = encode(&header, &claims, &encoding_key).unwrap();

        let req = Request::builder()
            .uri("/api/orders")
            .header(AUTHORIZATION, format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap();

        let response = service.ready().await.unwrap().call(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_auth_middleware_expired_token_rejected() {
        let (jwk, encoding_key) = sample_jwk();
        let client = JwksClient::new("https://auth.internal/.well_known/jwks.json".into()).unwrap();
        let cache = Cache::new_in_memory(client);
        cache.replace_local_jwks(vec![("a1b2c3d4e5f60718293a4b5c6d7e8f90".to_string(), jwk)]);

        let state = Arc::new(AuthState::new(cache));
        let mut service = AuthService { inner: DummyInner, state };

        let mut header = Header::new(Algorithm::HS256);
        header.kid = Some("a1b2c3d4e5f60718293a4b5c6d7e8f90".to_string());

        let now = current_timestamp();
        let claims = Claims {
            iss: "auth.internal".to_string(),
            sub: Uuid::new_v4(),
            iat: now - 7200,
            nbf: now - 7200,
            exp: now - 3600, // expired 1 hour ago
            aud: Audience::Order,
            role: UserRole::Client,
            jti: Uuid::new_v4(),
        };

        let token = encode(&header, &claims, &encoding_key).unwrap();

        let req = Request::builder()
            .uri("/api/orders")
            .header(AUTHORIZATION, format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap();

        let response = service.ready().await.unwrap().call(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn test_jwt_cache_hit_and_miss() {
        let cache = LocalJwtCache::new(Duration::from_secs(60), 100);
        let token = b"header.payload.signature";
        let now = Instant::now();

        assert_eq!(cache.get(token, now), None);

        let claims = Claims {
            iss: "auth.internal".to_string(),
            sub: Uuid::new_v4(),
            iat: 1000,
            nbf: 1000,
            exp: 2000,
            aud: Audience::Order,
            role: UserRole::Client,
            jti: Uuid::new_v4(),
        };

        cache.insert(token, claims.clone());
        let hit = cache.get(token, now).unwrap();
        assert_eq!(hit, claims);
    }

    #[test]
    fn test_jwt_cache_expiration() {
        let cache = LocalJwtCache::new(Duration::from_millis(10), 100);
        let token = b"header.payload.signature";

        let claims = Claims {
            iss: "auth.internal".to_string(),
            sub: Uuid::new_v4(),
            iat: 1000,
            nbf: 1000,
            exp: 2000,
            aud: Audience::Order,
            role: UserRole::Client,
            jti: Uuid::new_v4(),
        };

        cache.insert(token, claims);
        std::thread::sleep(Duration::from_millis(20));

        let now = Instant::now();
        assert_eq!(cache.get(token, now), None);
    }

    #[test]
    fn test_jwt_cache_evict() {
        let cache = LocalJwtCache::new(Duration::from_secs(60), 100);
        let token = b"header.payload.signature";
        let now = Instant::now();

        let claims = Claims {
            iss: "auth.internal".to_string(),
            sub: Uuid::new_v4(),
            iat: 1000,
            nbf: 1000,
            exp: 2000,
            aud: Audience::Order,
            role: UserRole::Client,
            jti: Uuid::new_v4(),
        };

        cache.insert(token, claims);
        assert!(cache.get(token, now).is_some());

        cache.evict(token);
        assert_eq!(cache.get(token, now), None);
    }

    #[test]
    fn test_jwt_cache_sharding_distribution() {
        let cache = LocalJwtCache::new(Duration::from_secs(60), 100);
        let mut active_shards = 0;

        for i in 0..200 {
            let token = format!("token_{i}");
            let claims = Claims {
                iss: "auth.internal".to_string(),
                sub: Uuid::new_v4(),
                iat: 1000,
                nbf: 1000,
                exp: 2000,
                aud: Audience::Order,
                role: UserRole::Client,
                jti: Uuid::new_v4(),
            };
            cache.insert(token.as_bytes(), claims);
        }

        for shard in cache.shards.iter() {
            if !shard.entries.read().is_empty() {
                active_shards += 1;
            }
        }

        assert!(active_shards > 30);
    }

    #[tokio::test]
    async fn test_auth_middleware_caches_valid_token_and_snoops_logout() {
        let (jwk, encoding_key) = sample_jwk();
        let client = JwksClient::new("https://auth.internal/.well_known/jwks.json".into()).unwrap();
        let cache = Cache::new_in_memory(client);
        cache.replace_local_jwks(vec![("a1b2c3d4e5f60718293a4b5c6d7e8f90".to_string(), jwk)]);

        let state = Arc::new(AuthState::new(cache));
        let mut service = AuthService {
            inner: DummyInner,
            state: state.clone(),
        };

        let mut header = Header::new(Algorithm::HS256);
        header.kid = Some("a1b2c3d4e5f60718293a4b5c6d7e8f90".to_string());

        let now = current_timestamp();
        let claims = Claims {
            iss: "auth.internal".to_string(),
            sub: Uuid::new_v4(),
            iat: now,
            nbf: now - 10,
            exp: now + 3600,
            aud: Audience::Order,
            role: UserRole::Client,
            jti: Uuid::new_v4(),
        };

        let token = encode(&header, &claims, &encoding_key).unwrap();

        let req1 = Request::builder()
            .uri("/api/v1/orders")
            .header(AUTHORIZATION, format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap();

        let res1 = service.ready().await.unwrap().call(req1).await.unwrap();
        assert_eq!(res1.status(), StatusCode::OK);
        assert_eq!(state.jwt_cache.len(), 1);

        let req2 = Request::builder()
            .uri("/api/v1/orders/123")
            .header(AUTHORIZATION, format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap();

        let res2 = service.ready().await.unwrap().call(req2).await.unwrap();
        assert_eq!(res2.status(), StatusCode::OK);

        let req_logout = Request::builder()
            .uri("/api/v1/auth/logout")
            .header(AUTHORIZATION, format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap();

        let res_logout = service.ready().await.unwrap().call(req_logout).await.unwrap();
        assert_eq!(res_logout.status(), StatusCode::OK);
        assert_eq!(state.jwt_cache.len(), 0);
    }

    #[test]
    fn test_jwt_cache_disabled_bypasses_cache() {
        let config = crate::config::JwtCacheConfig {
            enabled: false,
            ttl_secs: 15,
            max_entries_per_shard: 100,
            sweep_interval_ms: 100,
        };
        let cache = LocalJwtCache::from_config(&config);
        let claims = Claims {
            iss: "auth.internal".to_string(),
            sub: Uuid::new_v4(),
            iat: 1000,
            nbf: 1000,
            exp: 2000,
            aud: Audience::Order,
            role: UserRole::Client,
            jti: Uuid::new_v4(),
        };
        let token = b"test-token";
        cache.insert(token, claims);
        assert_eq!(cache.len(), 0);
        assert!(cache.get(token, Instant::now()).is_none());
    }
}
