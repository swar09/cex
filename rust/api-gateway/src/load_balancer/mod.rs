pub mod backend;
pub mod config;
pub mod control_plane;
pub mod data_plane;
pub mod error;
pub mod layer;
pub mod strategy;

#[cfg(test)]
pub mod tests;

pub use backend::{Backend, BackendGuard};
pub use config::{AlgorithmType, CircuitBreakerConfig, HealthCheckConfig, LoadBalancerConfig};
pub use control_plane::ControlPlane;
pub use data_plane::LoadBalancer;
pub use error::LoadBalancerError;
pub use layer::{LoadBalancerLayer, LoadBalancerRegistry, LoadBalancerService, SelectedBackend, resolve_service_name};
pub use strategy::{LoadBalancingStrategy, PowerOfTwoChoices, RoundRobin};
