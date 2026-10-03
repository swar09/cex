use std::{path::PathBuf, time::Duration};

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub bind_addr: String,
    pub ws_broadcast_capacity: usize,
    pub redis_auth_url: String,
    pub redis_replica_url: String,
    pub kafka_brokers: String,
    pub kafka_topic: String,
    pub wal_dir: PathBuf,
    pub wal_segment_size: usize,
    pub wal_flush_interval: Duration,
    pub ingress_buffer_size: usize,
    pub egress_buffer_size: usize,
    pub jwks_url: Option<String>,
    pub pinning_enabled: bool,
    pub core_wal: usize,
    pub core_engine: usize,
    pub core_kafka: usize,
    pub core_ws_publisher: usize,
    pub core_replica: usize,
    pub core_pruner: usize,
}

impl ServerConfig {
    pub fn from_env() -> Self {
        Self {
            bind_addr: env_str("GATEWAY_BIND_ADDR", "0.0.0.0:8080"),
            ws_broadcast_capacity: env_usize("WS_BROADCAST_CAPACITY", 4096),
            redis_auth_url: env_str("REDIS_AUTH_URL", &env_str("REDIS_URL", "redis://127.0.0.1:6379")),
            redis_replica_url: env_str("REDIS_READ_REPLICA_URL", &env_str("REDIS_URL", "redis://127.0.0.1:6379")),
            kafka_brokers: env_str("KAFKA_BROKERS", "127.0.0.1:9092"),
            kafka_topic: env_str("KAFKA_TOPIC", "orderbook.events.logs"),
            wal_dir: PathBuf::from(env_str("WAL_DIR", "data/wal")),
            wal_segment_size: env_usize("WAL_SEGMENT_SIZE", 16 * 1024 * 1024),
            wal_flush_interval: Duration::from_millis(env_u64("WAL_FLUSH_MS", 5)),
            ingress_buffer_size: env_usize("INGRESS_BUFFER_SIZE", 65536),
            egress_buffer_size: env_usize("EGRESS_BUFFER_SIZE", 65536),
            jwks_url: std::env::var("JWKS_URL").ok(),
            pinning_enabled: env_bool("PINNING_ENABLED", false),
            core_wal: env_usize("CORE_WAL", 1),
            core_engine: env_usize("CORE_ENGINE", 2),
            core_kafka: env_usize("CORE_KAFKA", 3),
            core_ws_publisher: env_usize("CORE_WS_PUBLISHER", 4),
            core_replica: env_usize("CORE_REPLICA", 5),
            core_pruner: env_usize("CORE_PRUNER", 6),
        }
    }
}

fn env_str(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

fn env_usize(key: &str, default: usize) -> usize {
    std::env::var(key).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

fn env_u64(key: &str, default: u64) -> u64 {
    std::env::var(key).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

fn env_bool(key: &str, default: bool) -> bool {
    match std::env::var(key).as_deref() {
        Ok("true" | "1" | "yes") => true,
        Ok("false" | "0" | "no") => false,
        _ => default,
    }
}
