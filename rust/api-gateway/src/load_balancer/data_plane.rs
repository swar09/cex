use std::sync::Arc;

use arc_swap::ArcSwap;

use crate::load_balancer::{
    backend::{Backend, BackendGuard},
    config::{AlgorithmType, CircuitBreakerConfig, LoadBalancerConfig},
    error::LoadBalancerError,
    strategy::{LoadBalancingStrategy, PowerOfTwoChoices, RoundRobin},
};

pub struct LoadBalancer {
    service_name: String,
    backends: ArcSwap<Vec<Arc<Backend>>>,
    strategy: Box<dyn LoadBalancingStrategy>,
    circuit_breaker_config: CircuitBreakerConfig,
}

impl LoadBalancer {
    pub fn new(config: LoadBalancerConfig) -> Result<Self, LoadBalancerError> {
        config.validate()?;

        let strategy: Box<dyn LoadBalancingStrategy> = match config.algorithm {
            AlgorithmType::PowerOfTwoLeastLoaded => Box::new(PowerOfTwoChoices),
            AlgorithmType::RoundRobin => Box::new(RoundRobin::default()),
        };

        let backends: Vec<Arc<Backend>> = config.initial_endpoints.into_iter().map(|addr| Arc::new(Backend::new(addr))).collect();

        Ok(Self {
            service_name: config.service_name,
            backends: ArcSwap::from_pointee(backends),
            strategy,
            circuit_breaker_config: config.circuit_breaker,
        })
    }

    pub fn select(&self) -> Result<(Arc<Backend>, BackendGuard), LoadBalancerError> {
        let backends_guard = self.backends.load();
        let selected = self.strategy.select(&self.service_name, &backends_guard)?;
        let guard = selected.acquire();
        Ok((Arc::clone(selected), guard))
    }

    pub fn swap_backends(&self, new_backends: Vec<Arc<Backend>>) {
        self.backends.store(Arc::new(new_backends));
    }

    pub fn mark_failure(&self, backend: &Backend) {
        backend.mark_transport_failure(self.circuit_breaker_config.consecutive_transport_failures_threshold);
    }

    pub fn mark_success(&self, backend: &Backend) {
        backend.mark_transport_success();
    }

    pub fn service_name(&self) -> &str {
        &self.service_name
    }

    pub fn backends(&self) -> arc_swap::Guard<Arc<Vec<Arc<Backend>>>> {
        self.backends.load()
    }
}
