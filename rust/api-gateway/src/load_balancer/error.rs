use std::net::SocketAddr;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum LoadBalancerError {
    #[error("No healthy backends available for service '{service_name}'")]
    NoHealthyBackends { service_name: String },

    #[error("Backend pool is empty for service '{service_name}'")]
    EmptyBackendPool { service_name: String },

    #[error("Transport connection failed to backend '{addr}': {source}")]
    TransportFailure {
        addr: SocketAddr,
        #[source]
        source: std::io::Error,
    },

    #[error("Health check timed out for backend '{addr}' after {timeout_ms}ms")]
    HealthCheckTimeout { addr: SocketAddr, timeout_ms: u64 },

    #[error("Invalid configuration: {reason}")]
    InvalidConfiguration { reason: String },
}
