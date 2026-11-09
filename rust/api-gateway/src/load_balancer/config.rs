use std::net::SocketAddr;

use crate::load_balancer::error::LoadBalancerError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AlgorithmType {
    #[default]
    PowerOfTwoLeastLoaded,
    RoundRobin,
}

#[derive(Debug, Clone)]
pub struct HealthCheckConfig {
    pub path: String,
    pub interval_ms: u64,
    pub timeout_ms: u64,
    pub consecutive_success_threshold: usize,
    pub consecutive_failure_threshold: usize,
}

impl Default for HealthCheckConfig {
    fn default() -> Self {
        Self {
            path: "/healthz".to_string(),
            interval_ms: 5000,
            timeout_ms: 2000,
            consecutive_success_threshold: 2,
            consecutive_failure_threshold: 3,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    pub consecutive_transport_failures_threshold: usize,
    pub half_open_probe_interval_ms: u64,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            consecutive_transport_failures_threshold: 3,
            half_open_probe_interval_ms: 5000,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LoadBalancerConfig {
    pub service_name: String,
    pub algorithm: AlgorithmType,
    pub health_check: HealthCheckConfig,
    pub circuit_breaker: CircuitBreakerConfig,
    pub initial_endpoints: Vec<SocketAddr>,
}

impl LoadBalancerConfig {
    pub fn validate(&self) -> Result<(), LoadBalancerError> {
        if self.service_name.trim().is_empty() {
            return Err(LoadBalancerError::InvalidConfiguration {
                reason: "service_name cannot be empty".to_string(),
            });
        }
        if !self.health_check.path.starts_with('/') {
            return Err(LoadBalancerError::InvalidConfiguration {
                reason: "health_check.path must start with '/'".to_string(),
            });
        }
        if self.health_check.interval_ms == 0 {
            return Err(LoadBalancerError::InvalidConfiguration {
                reason: "health_check.interval_ms must be > 0".to_string(),
            });
        }
        if self.health_check.timeout_ms == 0 {
            return Err(LoadBalancerError::InvalidConfiguration {
                reason: "health_check.timeout_ms must be > 0".to_string(),
            });
        }
        if self.health_check.consecutive_success_threshold < 1 {
            return Err(LoadBalancerError::InvalidConfiguration {
                reason: "health_check.consecutive_success_threshold must be >= 1".to_string(),
            });
        }
        if self.health_check.consecutive_failure_threshold < 1 {
            return Err(LoadBalancerError::InvalidConfiguration {
                reason: "health_check.consecutive_failure_threshold must be >= 1".to_string(),
            });
        }
        if self.circuit_breaker.consecutive_transport_failures_threshold < 1 {
            return Err(LoadBalancerError::InvalidConfiguration {
                reason: "circuit_breaker.consecutive_transport_failures_threshold must be >= 1".to_string(),
            });
        }
        Ok(())
    }
}
