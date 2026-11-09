use std::sync::{Arc, atomic::Ordering};

use crate::load_balancer::{backend::Backend, error::LoadBalancerError, strategy::LoadBalancingStrategy};

#[derive(Default)]
pub struct PowerOfTwoChoices;

impl LoadBalancingStrategy for PowerOfTwoChoices {
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

        if healthy.len() == 1 {
            return Ok(healthy[0]);
        }

        let idx1 = fastrand::usize(..healthy.len());
        let mut idx2 = fastrand::usize(..healthy.len());
        if idx1 == idx2 {
            idx2 = (idx1 + 1) % healthy.len();
        }

        let b1 = healthy[idx1];
        let b2 = healthy[idx2];

        if b1.in_flight.load(Ordering::Relaxed) <= b2.in_flight.load(Ordering::Relaxed) {
            Ok(b1)
        } else {
            Ok(b2)
        }
    }
}
