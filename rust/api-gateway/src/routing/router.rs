use std::sync::Arc;

use axum::{
    Router,
    response::{IntoResponse, Response},
    routing::{any, get, post},
};

use crate::middleware::{
    AuthLayer, AuthState, ContentLengthLimitLayer, HeaderSanitizeLayer, TimeoutLayer, TraceLayer,
};

pub fn build_router(auth_state: Arc<AuthState>) -> Router {
    let public_routes = Router::new()
        .route("/healthz", get(health_handler))
        .route("/api/v1/auth/login", post(stub_public_handler))
        .route("/api/v1/auth/register", post(stub_public_handler));

    let protected_routes = Router::new()
        .route("/api/v1/orders/{*path}", any(stub_protected_handler))
        .route("/api/v1/engine/{*path}", any(stub_protected_handler))
        .route("/api/v1/ledger/{*path}", any(stub_protected_handler))
        .route("/api/v1/user/{*path}", any(stub_protected_handler))
        .route("/api/v1/kyc/{*path}", any(stub_protected_handler))
        .layer(AuthLayer::new(auth_state));

    Router::new()
        .merge(public_routes)
        .merge(protected_routes)
        .layer(HeaderSanitizeLayer)
        .layer(ContentLengthLimitLayer::default())
        .layer(TimeoutLayer::default())
        .layer(TraceLayer)
}

async fn health_handler() -> Response {
    "OK".into_response()
}

async fn stub_public_handler() -> Response {
    "public stub".into_response()
}

async fn stub_protected_handler() -> Response {
    "protected stub".into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        extract::Request,
        http::{StatusCode, header::AUTHORIZATION},
    };
    use jsonwebtoken::{Algorithm, EncodingKey, Header, encode, jwk::Jwk};
    use tower::ServiceExt;
    use uuid::Uuid;
    use std::time::{SystemTime, UNIX_EPOCH};
    use crate::{
        cache::cache::{Cache, JwksClient},
        middleware::auth::{Audience, Claims, UserRole},
    };

    fn sample_jwk() -> (Jwk, EncodingKey) {
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

    fn test_auth_state() -> (Arc<AuthState>, Jwk, EncodingKey) {
        let (jwk, encoding_key) = sample_jwk();
        let client = JwksClient::new("https://auth.internal/.well_known/jwks.json".into()).unwrap();
        let cache = Cache::new_in_memory(client);
        cache.replace_local_jwks(vec![("a1b2c3d4e5f60718293a4b5c6d7e8f90".to_string(), jwk.clone())]);
        let state = Arc::new(AuthState::new(cache));
        (state, jwk, encoding_key)
    }

    #[tokio::test]
    async fn test_public_endpoint_accessible_without_auth() {
        let (state, _, _) = test_auth_state();
        let router = build_router(state);

        let req = Request::builder()
            .uri("/healthz")
            .body(Body::empty())
            .unwrap();

        let res = router.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        assert!(res.headers().contains_key("x-request-id"));
    }

    #[tokio::test]
    async fn test_protected_endpoint_rejected_without_auth() {
        let (state, _, _) = test_auth_state();
        let router = build_router(state);

        let req = Request::builder()
            .uri("/api/v1/orders/active")
            .body(Body::empty())
            .unwrap();

        let res = router.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
        assert!(res.headers().contains_key("x-request-id"));
    }

    #[tokio::test]
    async fn test_protected_endpoint_authorized_with_valid_token() {
        let (state, _, encoding_key) = test_auth_state();
        let router = build_router(state);

        let mut header = Header::new(Algorithm::HS256);
        header.kid = Some("a1b2c3d4e5f60718293a4b5c6d7e8f90".to_string());

        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
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

        let req = Request::builder()
            .uri("/api/v1/orders/active")
            .header(AUTHORIZATION, format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap();

        let res = router.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        assert!(res.headers().contains_key("x-request-id"));
    }

    #[tokio::test]
    async fn test_global_content_length_limit_enforced_on_public_route() {
        let (state, _, _) = test_auth_state();
        let router = build_router(state);

        let req = Request::builder()
            .uri("/healthz")
            .header("content-length", "20000000") // 20 MB > 10 MB limit
            .body(Body::empty())
            .unwrap();

        let res = router.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::PAYLOAD_TOO_LARGE);
    }
}
