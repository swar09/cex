use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use crate::load_balancer::{backend::Backend, error::LoadBalancerError, strategy::LoadBalancingStrategy};

#[derive(Default)]
pub struct RoundRobin {
    counter: AtomicUsize,
}

impl LoadBalancingStrategy for RoundRobin {
    fn select<'a>(&self, service_name: &str, backends: &'a [Arc<Backend>]) -> Result<&'a Arc<Backend>, LoadBalancerError> {
        if backends.is_empty() {
            return Err(LoadBalancerError::EmptyBackendPool {
                service_name: service_name.to_string(),
            });
        }

        let healthy: Vec<&'a Arc<Backend>> = backends.iter().filter(|b| b.is_healthy_and_open()).collect();

        if healthy.is_empty() {
            return Err(LoadBalancerError::NoHealthyBackends {
                service_name: service_name.to_string(),
            });
        }

        let idx = self.counter.fetch_add(1, Ordering::Relaxed) % healthy.len();
        Ok(healthy[idx])
    }
}
