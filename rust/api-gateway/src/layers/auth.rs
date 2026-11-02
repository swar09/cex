use std::{pin::Pin, sync::Arc};

use axum::{
    extract::Request,
    http::{HeaderName, HeaderValue, StatusCode, header::AUTHORIZATION},
    response::{IntoResponse, Response},
};
use jsonwebtoken::{DecodingKey, Validation, decode, decode_header};
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

/// Shared state for authentication layer.
#[derive(Clone)]
pub struct AuthState {
    /// In-memory JWKS cache and blacklist client.
    pub cache: Cache,
}

impl AuthState {
    /// Constructs new `AuthState` instance.
    pub fn new(cache: Cache) -> Self {
        Self { cache }
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

        let state = self.state.clone();
        let mut inner = self.inner.clone();

        Box::pin(async move {
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
            inner.call(req).await
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
}
