use std::{sync::Arc, time::Duration};

use axum::{
    Router,
    http::{
        HeaderName, Method,
        header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, ORIGIN},
    },
    response::{IntoResponse, Response},
    routing::{any, get},
};
use tower_http::cors::{AllowOrigin, CorsLayer};

use crate::{
    config::ServicesConfig,
    layers::{AuthLayer, AuthState, ContentLengthLimitLayer, HeaderSanitizeLayer, RateLimitLayer, RateLimiter, TimeoutLayer, TraceLayer},
    proxy::{ProxyClient, ProxyRouteProvider, reverse_proxy_handler},
};

#[derive(Clone)]
pub struct RouterState {
    pub auth: Arc<AuthState>,
    pub proxy: Arc<ProxyClient>,
    pub services: Arc<ServicesConfig>,
    pub rate_limiter: Option<Arc<RateLimiter>>,
    pub load_balancer: Option<crate::load_balancer::LoadBalancerRegistry>,
}

impl ProxyRouteProvider for RouterState {
    fn services(&self) -> &ServicesConfig {
        &self.services
    }

    fn proxy_client(&self) -> &ProxyClient {
        &self.proxy
    }
}

pub fn build_router(state: Arc<RouterState>) -> Router {
    let auth_layer = AuthLayer::new(state.auth.clone());

    let cors_layer = CorsLayer::new()
        .allow_origin(AllowOrigin::mirror_request())
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::PATCH, Method::DELETE, Method::OPTIONS])
        .allow_headers([AUTHORIZATION, CONTENT_TYPE, ACCEPT, ORIGIN, HeaderName::from_static("x-request-id")])
        .allow_credentials(true)
        .max_age(Duration::from_secs(86400));

    let auth_public_paths = [
        "/api/v1/auth/signup",
        "/api/v1/auth/register",
        "/api/v1/auth/login",
        "/api/v1/auth/oauth/google",
        "/api/v1/auth/oauth/google/callback",
    ];

    let auth_protected_paths = ["/api/v1/auth/logout", "/api/v1/auth/password/update", "/api/v1/auth/password/verify"];

    let service_prefixes = ["/api/v1/orders", "/api/v1/engine", "/api/v1/ledger", "/api/v1/user", "/api/v1/kyc"];

    let proxy_any = any(reverse_proxy_handler::<RouterState>);

    let mut public_routes = Router::new()
        .route("/healthz", get(health_handler))
        .route("/.well_known/jwks.json", proxy_any.clone());

    for path in auth_public_paths {
        public_routes = public_routes.route(path, proxy_any.clone());
    }

    let mut protected_routes = Router::new();
    for path in auth_protected_paths {
        protected_routes = protected_routes.route(path, proxy_any.clone());
    }
    for prefix in service_prefixes {
        protected_routes = protected_routes
            .route(prefix, proxy_any.clone())
            .route(&format!("{prefix}/{{*path}}"), proxy_any.clone());
    }
    let lb_layer = state
        .load_balancer
        .as_ref()
        .map(|lb| crate::load_balancer::LoadBalancerLayer::new(lb.clone()));

    let public_routes = if let Some(ref lb) = lb_layer {
        public_routes.layer(lb.clone())
    } else {
        public_routes
    };

    let protected_routes = if let Some(ref lb) = lb_layer {
        protected_routes.layer(lb.clone())
    } else {
        protected_routes
    };
    let protected_routes = if let Some(ref limiter) = state.rate_limiter {
        protected_routes.layer(RateLimitLayer::new(limiter.clone()))
    } else {
        protected_routes
    };
    let protected_routes = protected_routes.layer(auth_layer);

    let router = Router::new().merge(public_routes).merge(protected_routes);

    router
        .layer(HeaderSanitizeLayer)
        .layer(ContentLengthLimitLayer::default())
        .layer(TimeoutLayer::default())
        .layer(TraceLayer)
        .layer(cors_layer)
        .with_state(state)
}

async fn health_handler() -> Response {
    "OK".into_response()
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        str::FromStr,
        time::{SystemTime, UNIX_EPOCH},
    };

    use axum::{
        body::Body,
        extract::Request,
        http::{StatusCode, header::AUTHORIZATION},
    };
    use http_body_util::BodyExt;
    use jsonwebtoken::{Algorithm, EncodingKey, Header, encode, jwk::Jwk};
    use tower::ServiceExt;
    use uuid::Uuid;

    use super::*;
    use crate::{
        cache::cache::{Cache, JwksClient},
        layers::auth::{Audience, Claims, UserRole},
    };

    async fn spawn_mock_upstream() -> (String, tokio::sync::oneshot::Sender<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();

        let app = axum::Router::new().fallback(|req: axum::extract::Request| async move {
            let mut res = axum::response::Response::builder().status(axum::http::StatusCode::OK);
            for (k, v) in req.headers() {
                res = res.header(k, v);
            }
            res.body(req.into_body()).unwrap()
        });

        tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async move {
                    let _ = rx.await;
                })
                .await
                .ok();
        });

        (format!("http://127.0.0.1:{}", addr.port()), tx)
    }

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

    fn test_router_state(mock_url: Option<String>) -> (Arc<RouterState>, Jwk, EncodingKey) {
        let (jwk, encoding_key) = sample_jwk();
        let client = JwksClient::new("https://auth.internal/.well_known/jwks.json".into()).unwrap();
        let cache = Cache::new_in_memory(client);
        cache.replace_local_jwks(vec![("a1b2c3d4e5f60718293a4b5c6d7e8f90".to_string(), jwk.clone())]);
        let auth = Arc::new(AuthState::new(cache));
        let proxy = Arc::new(ProxyClient::new());
        let mut services = ServicesConfig::default();
        if let Some(url) = mock_url {
            services.orders = url.clone();
            services.engine = url.clone();
            services.ledger = url.clone();
            services.user = url.clone();
            services.kyc = url.clone();
            services.auth = url;
        }
        let state = Arc::new(RouterState {
            auth,
            proxy,
            services: Arc::new(services),
            rate_limiter: None,
            load_balancer: None,
        });
        (state, jwk, encoding_key)
    }

    #[tokio::test]
    async fn test_public_endpoint_accessible_without_auth() {
        let (state, _, _) = test_router_state(None);
        let router = build_router(state);

        let req = Request::builder().uri("/healthz").body(Body::empty()).unwrap();

        let res = router.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        assert!(res.headers().contains_key("x-request-id"));
    }

    #[tokio::test]
    async fn test_protected_endpoint_rejected_without_auth() {
        let (state, _, _) = test_router_state(None);
        let router = build_router(state);

        let req = Request::builder().uri("/api/v1/orders/active").body(Body::empty()).unwrap();

        let res = router.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
        assert!(res.headers().contains_key("x-request-id"));
    }

    #[tokio::test]
    async fn test_protected_endpoint_authorized_with_valid_token() {
        let (mock_url, _shutdown) = spawn_mock_upstream().await;
        let (state, _, encoding_key) = test_router_state(Some(mock_url));
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
        let (state, _, _) = test_router_state(None);
        let router = build_router(state);

        let req = Request::builder()
            .uri("/healthz")
            .header("content-length", "20000000") // 20 MB > 10 MB limit
            .body(Body::empty())
            .unwrap();

        let res = router.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::PAYLOAD_TOO_LARGE);
    }

    #[tokio::test]
    async fn test_pipeline_sanitizes_headers_and_mints_uuid_v7() {
        let (state, _, _) = test_router_state(None);
        let router = build_router(state);

        let incoming_spoofed_id = "11111111-1111-1111-1111-111111111111";
        let req = Request::builder()
            .uri("/healthz")
            .header("x-request-id", incoming_spoofed_id)
            .header("x-forwarded-for", "203.0.113.195")
            .header("x-user-id", "attacker-impersonated")
            .header("if-none-match", "etag-1234")
            .body(Body::empty())
            .unwrap();

        let res = router.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        let res_req_id = res.headers().get("x-request-id").expect("x-request-id header must exist");
        let parsed = Uuid::from_str(res_req_id.to_str().unwrap()).unwrap();
        assert_ne!(parsed.to_string(), incoming_spoofed_id);
        assert_eq!(parsed.get_version_num(), 7);
    }

    #[tokio::test]
    async fn test_pipeline_purges_misc_client_headers_and_injects_authentic_claims() {
        let (mock_url, _shutdown) = spawn_mock_upstream().await;
        let (state, _, encoding_key) = test_router_state(Some(mock_url));
        let router = build_router(state);

        let mut header = Header::new(Algorithm::HS256);
        header.kid = Some("a1b2c3d4e5f60718293a4b5c6d7e8f90".to_string());

        let user_id = Uuid::new_v4();
        let jti = Uuid::new_v4();
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        let claims = Claims {
            iss: "auth.internal".to_string(),
            sub: user_id,
            iat: now,
            nbf: now - 10,
            exp: now + 3600,
            aud: Audience::Order,
            role: UserRole::Client,
            jti,
        };

        let token = encode(&header, &claims, &encoding_key).unwrap();

        let req = Request::builder()
            .uri("/api/v1/orders/trade")
            .header(AUTHORIZATION, format!("Bearer {token}"))
            .header("x-user-id", "attacker-impersonated-user")
            .header("x-user-role", "Admin")
            .header("x-token-id", "attacker-impersonated-token")
            .header("x-forwarded-for", "203.0.113.195")
            .header("if-none-match", "W/\"etag-99\"")
            .header("cache-control", "max-age=0")
            .header("cookie", "session=evil")
            .header("x-request-id", "11111111-1111-1111-1111-111111111111")
            .body(Body::empty())
            .unwrap();

        let res = router.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        assert_eq!(res.headers().get("x-user-id").unwrap(), user_id.to_string().as_str());
        assert_eq!(res.headers().get("x-user-role").unwrap(), "client");
        assert_eq!(res.headers().get("x-token-id").unwrap(), jti.to_string().as_str());

        assert!(res.headers().get("authorization").is_none());
        assert!(res.headers().get("if-none-match").is_none());
        assert!(res.headers().get("cache-control").is_none());
        assert!(res.headers().get("cookie").is_none());
        assert!(res.headers().get("x-forwarded-for").is_none());

        let req_id_str = res.headers().get("x-request-id").unwrap().to_str().unwrap();
        assert_ne!(req_id_str, "11111111-1111-1111-1111-111111111111");
        let parsed_req_id = Uuid::from_str(req_id_str).unwrap();
        assert_eq!(parsed_req_id.get_version_num(), 7);
    }

    #[tokio::test]
    async fn test_reverse_proxy_zero_copy_body_and_path_query_forwarding() {
        let (mock_url, _shutdown) = spawn_mock_upstream().await;
        let (state, _, encoding_key) = test_router_state(Some(mock_url));
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
        let payload = r#"{"order_id":123,"symbol":"BTC/USDT","amount":"1.5"}"#;

        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/orders/create?mode=immediate")
            .header(AUTHORIZATION, format!("Bearer {token}"))
            .header("content-type", "application/json")
            .body(Body::from(payload))
            .unwrap();

        let res = router.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        let body_bytes = res.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(body_bytes, payload.as_bytes());
    }

    #[tokio::test]
    async fn test_reverse_proxy_upstream_down_returns_bad_gateway() {
        let unreachable_url = "http://127.0.0.1:1".to_string();
        let (state, _, encoding_key) = test_router_state(Some(unreachable_url));
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
            .method("GET")
            .uri("/api/v1/orders/status")
            .header(AUTHORIZATION, format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap();

        let res = router.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::BAD_GATEWAY);
    }

    #[tokio::test]
    async fn test_cors_preflight_options_request_returns_success_and_headers() {
        let (state, _, _) = test_router_state(None);
        let router = build_router(state);

        let req = Request::builder()
            .method("OPTIONS")
            .uri("/api/v1/orders/active")
            .header("origin", "https://app.cex.io")
            .header("access-control-request-method", "POST")
            .header("access-control-request-headers", "authorization,content-type")
            .body(Body::empty())
            .unwrap();

        let res = router.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(res.headers().get("access-control-allow-origin").unwrap(), "https://app.cex.io");
        assert_eq!(res.headers().get("access-control-allow-credentials").unwrap(), "true");
        assert!(res.headers().contains_key("access-control-allow-methods"));
        assert!(res.headers().contains_key("access-control-allow-headers"));
    }

    #[tokio::test]
    async fn test_cors_simple_request_returns_allow_origin_header() {
        let (state, _, _) = test_router_state(None);
        let router = build_router(state);

        let req = Request::builder()
            .method("GET")
            .uri("/healthz")
            .header("origin", "https://app.cex.io")
            .body(Body::empty())
            .unwrap();

        let res = router.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(res.headers().get("access-control-allow-origin").unwrap(), "https://app.cex.io");
    }

    #[tokio::test]
    async fn test_protected_endpoint_rate_limited_returns_429() {
        let (mock_url, _tx) = spawn_mock_upstream().await;
        let (state, _, encoding_key) = test_router_state(Some(mock_url));

        let user_id = Uuid::new_v4();
        let tiers = crate::config::TiersConfig {
            tier3: crate::config::TierLimitConfig {
                initial_tokens: 0,
                fill_rate: 0,
                burst_interval_secs: 30,
                max_tokens: 250,
                lease_batch_size: 10,
            },
            ..Default::default()
        };

        let mut role_mapping = HashMap::new();
        role_mapping.insert("client".to_string(), crate::config::Tier::Tier3);

        let limiter = Arc::new(RateLimiter::new(
            Arc::new(crate::config::RateLimiterConfig {
                enabled: true,
                redis_url: "redis://127.0.0.1:16379".into(),
                key_prefix: "rl".into(),
                lease_duration_secs: 5,
                batch_window_micros: 100,
                default_tier: crate::config::Tier::Tier4,
                tiers,
                role_mapping,
            }),
            crate::layers::RateLimiterEngine::Autonomous(crate::layers::AutonomousLimiter::new()),
        ));

        let state = Arc::new(RouterState {
            auth: state.auth.clone(),
            proxy: state.proxy.clone(),
            services: state.services.clone(),
            rate_limiter: Some(limiter),
            load_balancer: None,
        });

        let router = build_router(state);

        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        let claims = Claims {
            iss: "https://auth.internal".to_string(),
            sub: user_id,
            iat: now,
            nbf: now,
            exp: now + 3600,
            aud: Audience::Order,
            role: UserRole::Client,
            jti: Uuid::new_v4(),
        };

        let mut header = Header::new(Algorithm::HS256);
        header.kid = Some("a1b2c3d4e5f60718293a4b5c6d7e8f90".to_string());
        let token = encode(&header, &claims, &encoding_key).unwrap();

        let req = Request::builder()
            .method("GET")
            .uri("/api/v1/orders/123")
            .header(AUTHORIZATION, format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap();

        let res = router.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(res.headers().get("retry-after").unwrap(), "30");
        assert_eq!(res.headers().get("x-ratelimit-remaining").unwrap(), "0");
    }

    #[tokio::test]
    async fn test_router_with_load_balancer_routes_to_backend() {
        let (mock_url, _shutdown) = spawn_mock_upstream().await;
        let (state, _, encoding_key) = test_router_state(None);
        let addr = mock_url.trim_start_matches("http://").parse::<std::net::SocketAddr>().unwrap();
        let lb_config = crate::load_balancer::LoadBalancerConfig {
            service_name: "orders".to_string(),
            algorithm: crate::load_balancer::AlgorithmType::RoundRobin,
            health_check: crate::load_balancer::HealthCheckConfig::default(),
            circuit_breaker: crate::load_balancer::CircuitBreakerConfig::default(),
            initial_endpoints: vec![addr],
        };
        let lb = Arc::new(crate::load_balancer::LoadBalancer::new(lb_config).unwrap());
        let mut services = std::collections::HashMap::new();
        services.insert("orders".to_string(), lb);
        let registry = crate::load_balancer::LoadBalancerRegistry::new(services);

        let state = Arc::new(RouterState {
            auth: state.auth.clone(),
            proxy: state.proxy.clone(),
            services: state.services.clone(),
            rate_limiter: None,
            load_balancer: Some(registry),
        });

        let router = build_router(state);

        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        let claims = Claims {
            iss: "https://auth.internal".to_string(),
            sub: Uuid::new_v4(),
            iat: now,
            nbf: now,
            exp: now + 3600,
            aud: Audience::Order,
            role: UserRole::Client,
            jti: Uuid::new_v4(),
        };

        let mut header = Header::new(Algorithm::HS256);
        header.kid = Some("a1b2c3d4e5f60718293a4b5c6d7e8f90".to_string());
        let token = encode(&header, &claims, &encoding_key).unwrap();

        let req = Request::builder()
            .method("GET")
            .uri("/api/v1/orders/123")
            .header(AUTHORIZATION, format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap();

        let res = router.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_router_unauthenticated_request_rejected_before_load_balancer() {
        let (state, _, _) = test_router_state(None);
        // Create an LB for orders with 0 endpoints (which would yield 503 if
        // invoked)
        let lb_config = crate::load_balancer::LoadBalancerConfig {
            service_name: "orders".to_string(),
            algorithm: crate::load_balancer::AlgorithmType::RoundRobin,
            health_check: crate::load_balancer::HealthCheckConfig::default(),
            circuit_breaker: crate::load_balancer::CircuitBreakerConfig::default(),
            initial_endpoints: vec![],
        };
        let lb = Arc::new(crate::load_balancer::LoadBalancer::new(lb_config).unwrap());
        let mut services = std::collections::HashMap::new();
        services.insert("orders".to_string(), lb);
        let registry = crate::load_balancer::LoadBalancerRegistry::new(services);

        let state = Arc::new(RouterState {
            auth: state.auth.clone(),
            proxy: state.proxy.clone(),
            services: state.services.clone(),
            rate_limiter: None,
            load_balancer: Some(registry),
        });

        let router = build_router(state);

        let req = Request::builder().method("GET").uri("/api/v1/orders/123").body(Body::empty()).unwrap();

        let res = router.oneshot(req).await.unwrap();
        // Since AuthLayer runs before LoadBalancerLayer, it must reject with
        // 401, not 503!
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    }
}
