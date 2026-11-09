pub mod p2c_least_loaded;
pub mod round_robin;

use std::sync::Arc;

pub use p2c_least_loaded::PowerOfTwoChoices;
pub use round_robin::RoundRobin;

use crate::load_balancer::{backend::Backend, error::LoadBalancerError};

pub trait LoadBalancingStrategy: Send + Sync {
    fn select<'a>(&self, service_name: &str, backends: &'a [Arc<Backend>]) -> Result<&'a Arc<Backend>, LoadBalancerError>;
}
