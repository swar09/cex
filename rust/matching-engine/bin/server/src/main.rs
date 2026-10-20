use std::{fs, sync::Arc};

use axum::Router;
use cache::Cache;
use gateway::{
    AppState,
    routers::{v1_auth_routes, v1_health_routes, v1_order_routes, v1_ws_routes},
};
use tokio::net::TcpListener;

mod config;
mod pipeline;
mod spawner;

#[tokio::main]
async fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let cfg = config::ServerConfig::from_env();

    fs::create_dir_all(&cfg.wal_dir).expect("failed to create WAL directory");

    log::info!("Redis auth:    {}", cfg.redis_auth_url);
    log::info!("Redis replica: {}", cfg.redis_replica_url);
    log::info!("Kafka brokers: {}", cfg.kafka_brokers);

    let cache = Arc::new(
        Cache::new(&cfg.redis_auth_url, &cfg.redis_replica_url, cfg.jwks_url.clone())
            .await
            .expect("failed to initialise Redis cache"),
    );

    let pipelines = pipeline::build(&cfg);
    let event_tx = pipelines.event_tx.clone();
    let exchange_client = pipelines.exchange_client.clone();

    let _workers = spawner::spawn_all(&cfg, pipelines, event_tx.clone());

    let state = AppState::new(event_tx, cache, exchange_client, cfg.ws_broadcast_capacity);

    let app = Router::new()
        .merge(v1_health_routes().await)
        .merge(v1_auth_routes().await)
        .merge(v1_order_routes().await)
        .merge(v1_ws_routes().await)
        .nest(
            "/v1",
            Router::new()
                .merge(v1_health_routes().await)
                .merge(v1_auth_routes().await)
                .merge(v1_order_routes().await)
                .merge(v1_ws_routes().await),
        )
        .with_state(state);

    let listener = TcpListener::bind(&cfg.bind_addr).await.expect("failed to bind TCP listener");

    log::info!("CEX gateway listening on {}", cfg.bind_addr);

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("axum server error");

    log::info!("Server shut down gracefully");
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c().await.expect("failed to listen for ctrl-c");
    log::info!("Shutdown signal received");
}
