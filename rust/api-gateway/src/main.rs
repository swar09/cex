use std::sync::Arc;
use tokio::net::TcpListener;

use api_gateway::{
    cache::cache::{Cache, JwksClient},
    config::GatewayConfig,
    layers::AuthState,
    routing::build_router,
};

#[tokio::main]
async fn main() {
    let config = GatewayConfig::load_or_default();
    config.apply_to_env();

    let jwks_client = match JwksClient::new(config.auth.jwks_endpoint.clone()) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[gateway] invalid jwks endpoint in config: {e}");
            return;
        }
    };

    let cache = match Cache::new(&config.redis.url, jwks_client).await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[gateway] redis connection unavailable, running in-memory: {e}");
            let fallback_client =
                JwksClient::new(config.auth.jwks_endpoint).expect("valid jwks endpoint");
            Cache::new_in_memory(fallback_client)
        }
    };

    let auth_state = Arc::new(AuthState::new(cache));
    let app = build_router(auth_state);

    let bind_addr = format!("{}:{}", config.server.host, config.server.port);
    let listener = TcpListener::bind(&bind_addr)
        .await
        .unwrap_or_else(|_| panic!("Failed to bind to {bind_addr}"));

    println!("[gateway] running on http://{bind_addr}");
    axum::serve(listener, app).await.expect("server error");
}
