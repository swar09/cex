use std::sync::Arc;

use uuid::Uuid;

use crate::config::{RateLimiterConfig, Tier, TierLimitConfig};
pub use crate::layers::rate_limit::lease::{Decision, RateLimitError, RateLimiterEngine};

pub struct RateLimiter {
    pub config: Arc<RateLimiterConfig>,
    pub engine: RateLimiterEngine,
}

impl RateLimiter {
    pub fn new(config: Arc<RateLimiterConfig>, engine: RateLimiterEngine) -> Self {
        Self { config, engine }
    }

    #[inline]
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    #[inline]
    pub fn resolve_role(&self, role: Option<&str>) -> Tier {
        role.and_then(|r| self.config.role_mapping.get(r).copied())
            .unwrap_or(self.config.default_tier)
    }

    #[inline]
    pub fn tier_config(&self, tier: Tier) -> &TierLimitConfig {
        self.config.tiers.get(tier)
    }

    pub async fn check(&self, user_id: &Uuid, tier: Tier) -> Result<Decision, RateLimitError> {
        if !self.config.enabled {
            return Ok(Decision::Allowed {
                remaining: u64::MAX,
                limit: u64::MAX,
            });
        }

        let tier_config = self.config.tiers.get(tier);
        self.engine.check(user_id, tier, tier_config).await
    }

    pub fn clear(&self) {
        self.engine.clear();
    }
}
