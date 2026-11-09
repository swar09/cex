use std::sync::Arc;

use crate::layers::rate_limit::{limiter::RateLimiter, service::RateLimitService};

#[derive(Clone)]
pub struct RateLimitLayer {
    pub limiter: Arc<RateLimiter>,
}

impl RateLimitLayer {
    pub fn new(limiter: Arc<RateLimiter>) -> Self {
        Self { limiter }
    }
}

impl<S> tower::Layer<S> for RateLimitLayer {
    type Service = RateLimitService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        RateLimitService {
            inner,
            limiter: self.limiter.clone(),
        }
    }
}
