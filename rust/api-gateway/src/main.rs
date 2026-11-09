use std::sync::Arc;

use api_gateway::{
    cache::cache::{Cache, JwksClient},
    config::GatewayConfig,
    layers::{AuthState, AutonomousLimiter, DistributedLimiter, RateLimiter, RateLimiterEngine, RedisCoordinator},
    proxy::ProxyClient,
    routing::{RouterState, build_router},
};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() {
    let config = GatewayConfig::load_or_default();
    config.apply_to_env();

    let jwks_client = match JwksClient::new(config.auth.jwks_endpoint.clone()) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[gateway] invalid jwks endpoint in config: {e}");
            return;
        },
    };

    let cache = match Cache::new(&config.redis.url, jwks_client).await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[gateway] redis connection unavailable, running in-memory: {e}");
            let fallback_client = JwksClient::new(config.auth.jwks_endpoint).expect("valid jwks endpoint");
            Cache::new_in_memory(fallback_client)
        },
    };

    let rate_limiter = if config.rate_limiter.enabled {
        match RedisCoordinator::new(&config.rate_limiter.redis_url, config.rate_limiter.key_prefix.clone()).await {
            Ok(coordinator) => {
                let lease_duration = std::time::Duration::from_secs(config.rate_limiter.lease_duration_secs);
                let batch_window = std::time::Duration::from_micros(config.rate_limiter.batch_window_micros);
                let distributed = DistributedLimiter::new(Arc::new(coordinator), lease_duration, batch_window);
                Some(Arc::new(RateLimiter::new(
                    Arc::new(config.rate_limiter.clone()),
                    RateLimiterEngine::Distributed(distributed),
                )))
            },
            Err(e) => {
                eprintln!("[gateway] rate limiter redis connection failed, falling back to autonomous mode: {e}");
                Some(Arc::new(RateLimiter::new(
                    Arc::new(config.rate_limiter.clone()),
                    RateLimiterEngine::Autonomous(AutonomousLimiter::new()),
                )))
            },
        }
    } else {
        None
    };

    let auth_state = Arc::new(AuthState::from_config(cache, &config.jwt_cache));
    let proxy_client = Arc::new(ProxyClient::new());
    let services_config = Arc::new(config.services);
    let router_state = Arc::new(RouterState {
        auth: auth_state,
        proxy: proxy_client,
        services: services_config,
        rate_limiter,
        load_balancer: None,
    });
    let app = build_router(router_state);

    let bind_addr = format!("{}:{}", config.server.host, config.server.port);
    let listener = TcpListener::bind(&bind_addr)
        .await
        .unwrap_or_else(|_| panic!("Failed to bind to {bind_addr}"));

    println!("[gateway] running on http://{bind_addr}");
    axum::serve(listener, app).await.expect("server error");
}
