use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use fxhash::FxHashMap;
use parking_lot::Mutex;
use redis::{
    Client, Script,
    aio::{ConnectionManager, ConnectionManagerConfig},
};
use thiserror::Error;
use uuid::Uuid;

use crate::config::{Tier, TierLimitConfig};

#[derive(Debug, Error)]
pub enum RateLimitError {
    #[error("Redis error: {0}")]
    Redis(#[from] redis::RedisError),
    #[error("Internal error: {0}")]
    Internal(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Allowed { remaining: u64, limit: u64 },
    Throttled { retry_after_secs: u64, limit: u64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LeaseGrant {
    pub granted: u64,
    pub retry_after_secs: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RateLimitEntry {
    pub tier: Tier,
    pub quota: u64,
    pub last_fill: Instant,
    pub last_burst: Instant,
}

impl RateLimitEntry {
    #[inline]
    pub fn new(tier: Tier, quota: u64, now: Instant) -> Self {
        Self {
            tier,
            quota,
            last_fill: now,
            last_burst: now,
        }
    }
}

const NUM_SHARDS: usize = 64;

#[derive(Clone)]
pub struct AutonomousLimiter {
    shards: Arc<[Mutex<FxHashMap<Uuid, RateLimitEntry>>]>,
}

impl AutonomousLimiter {
    pub fn new() -> Self {
        let shards: Arc<[Mutex<FxHashMap<Uuid, RateLimitEntry>>]> = (0..NUM_SHARDS)
            .map(|_| Mutex::new(FxHashMap::default()))
            .collect::<Vec<_>>()
            .into_boxed_slice()
            .into();

        let limiter = Self { shards };
        limiter.spawn_cleaner();
        limiter
    }

    #[inline]
    fn shard(&self, user_id: &Uuid) -> &Mutex<FxHashMap<Uuid, RateLimitEntry>> {
        let idx = (user_id.as_u128() as usize) % NUM_SHARDS;
        &self.shards[idx]
    }

    fn spawn_cleaner(&self) {
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            let shards = Arc::clone(&self.shards);
            handle.spawn(async move {
                let mut interval = tokio::time::interval(Duration::from_secs(60));
                loop {
                    interval.tick().await;
                    let now = Instant::now();
                    let ttl = Duration::from_secs(300);
                    for shard in shards.iter() {
                        let mut guard = shard.lock();
                        guard.retain(|_, entry| now.duration_since(entry.last_fill) < ttl);
                    }
                }
            });
        }
    }

    pub fn prune_stale(&self, now: Instant, ttl: Duration) {
        for shard in self.shards.iter() {
            let mut guard = shard.lock();
            guard.retain(|_, entry| now.duration_since(entry.last_fill) < ttl);
        }
    }

    pub fn len(&self) -> usize {
        self.shards.iter().map(|s| s.lock().len()).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn check(&self, user_id: &Uuid, tier: Tier, config: &TierLimitConfig) -> Decision {
        let now = Instant::now();
        let shard = self.shard(user_id);
        let mut guard = shard.lock();

        let entry = guard
            .entry(*user_id)
            .or_insert_with(|| RateLimitEntry::new(tier, config.initial_tokens, now));

        entry.tier = tier;

        let elapsed_fill = now.duration_since(entry.last_fill).as_secs();
        if elapsed_fill > 0 {
            entry.quota = entry.quota.saturating_add(elapsed_fill * config.fill_rate);
            entry.last_fill = now;
        }

        let elapsed_burst = now.duration_since(entry.last_burst).as_secs();
        if elapsed_burst >= config.burst_interval_secs {
            let bursts = elapsed_burst / config.burst_interval_secs;
            entry.quota = entry.quota.saturating_add(bursts * config.initial_tokens);
            entry.last_burst = now;
        }

        entry.quota = entry.quota.min(config.max_tokens);

        if entry.quota > 0 {
            entry.quota -= 1;
            Decision::Allowed {
                remaining: entry.quota,
                limit: config.max_tokens,
            }
        } else {
            let retry_after = if config.fill_rate > 0 { 1 } else { config.burst_interval_secs };
            Decision::Throttled {
                retry_after_secs: retry_after,
                limit: config.max_tokens,
            }
        }
    }

    pub fn clear(&self) {
        for shard in self.shards.iter() {
            let mut guard = shard.lock();
            guard.clear();
        }
    }
}

impl Default for AutonomousLimiter {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LeaseEntry {
    pub tier: Tier,
    pub available: u64,
    pub expiry: Instant,
    pub max_limit: u64,
}

impl LeaseEntry {
    #[inline]
    pub fn new(tier: Tier, available: u64, expiry: Instant, max_limit: u64) -> Self {
        Self {
            tier,
            available,
            expiry,
            max_limit,
        }
    }

    #[inline]
    pub fn is_expired(&self, now: Instant) -> bool {
        now >= self.expiry
    }

    #[inline]
    pub fn try_consume(&mut self, now: Instant) -> Option<u64> {
        if self.is_expired(now) || self.available == 0 {
            return None;
        }
        self.available -= 1;
        Some(self.available)
    }

    #[inline]
    pub fn update_lease(&mut self, tier: Tier, available: u64, expiry: Instant, max_limit: u64) {
        self.tier = tier;
        self.available = available;
        self.expiry = expiry;
        self.max_limit = max_limit;
    }
}

static RATE_LIMIT_LUA: &str = include_str!("../../../../../infra/redis-rate-limiter/scripts/rate_limit.lua");

pub struct RedisCoordinator {
    conn: ConnectionManager,
    key_prefix: String,
    script: Script,
}

impl RedisCoordinator {
    pub async fn new(redis_url: &str, key_prefix: String) -> Result<Self, RateLimitError> {
        let client = Client::open(redis_url)?;
        let config = ConnectionManagerConfig::new()
            .set_connection_timeout(Some(Duration::from_secs(2)))
            .set_response_timeout(Some(Duration::from_secs(1)));
        let conn = ConnectionManager::new_with_config(client, config).await?;

        let script = Script::new(RATE_LIMIT_LUA);
        Ok(Self { conn, key_prefix, script })
    }

    pub async fn lease_quota(&self, user_id: &Uuid, _tier: Tier, config: &TierLimitConfig) -> Result<LeaseGrant, RateLimitError> {
        let key = format!("{}:user:{}", self.key_prefix, user_id);
        let ttl = config.burst_interval_secs.max(60) * 2;
        let mut conn = self.conn.clone();

        let result: Vec<u64> = self
            .script
            .key(&key)
            .arg(config.initial_tokens)
            .arg(config.fill_rate)
            .arg(config.burst_interval_secs)
            .arg(config.max_tokens)
            .arg(config.lease_batch_size)
            .arg(ttl)
            .invoke_async(&mut conn)
            .await?;

        let granted = result.first().copied().unwrap_or(0);
        let retry_after_secs = result.get(1).copied().unwrap_or(0);

        Ok(LeaseGrant { granted, retry_after_secs })
    }
}

#[derive(Clone)]
pub struct DistributedLimiter {
    shards: Arc<[Mutex<FxHashMap<Uuid, LeaseEntry>>]>,
    coordinator: Arc<RedisCoordinator>,
    fallback: AutonomousLimiter,
    lease_duration: Duration,
    batch_window: Duration,
    redis_cooldown_until: Arc<Mutex<Option<Instant>>>,
}

impl DistributedLimiter {
    pub fn new(coordinator: Arc<RedisCoordinator>, lease_duration: Duration, batch_window: Duration) -> Self {
        let shards: Arc<[Mutex<FxHashMap<Uuid, LeaseEntry>>]> = (0..NUM_SHARDS)
            .map(|_| Mutex::new(FxHashMap::default()))
            .collect::<Vec<_>>()
            .into_boxed_slice()
            .into();

        let limiter = Self {
            shards,
            coordinator,
            fallback: AutonomousLimiter::new(),
            lease_duration,
            batch_window,
            redis_cooldown_until: Arc::new(Mutex::new(None)),
        };
        limiter.spawn_cleaner();
        limiter
    }

    #[inline]
    pub fn batch_window(&self) -> Duration {
        self.batch_window
    }

    #[inline]
    pub fn lease_duration(&self) -> Duration {
        self.lease_duration
    }

    #[inline]
    fn shard(&self, user_id: &Uuid) -> &Mutex<FxHashMap<Uuid, LeaseEntry>> {
        let idx = (user_id.as_u128() as usize) % NUM_SHARDS;
        &self.shards[idx]
    }

    fn spawn_cleaner(&self) {
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            let shards = Arc::clone(&self.shards);
            handle.spawn(async move {
                let mut interval = tokio::time::interval(Duration::from_secs(30));
                loop {
                    interval.tick().await;
                    let now = Instant::now();
                    for shard in shards.iter() {
                        let mut guard = shard.lock();
                        guard.retain(|_, lease| !lease.is_expired(now));
                    }
                }
            });
        }
    }

    pub fn prune_expired(&self, now: Instant) {
        for shard in self.shards.iter() {
            let mut guard = shard.lock();
            guard.retain(|_, lease| !lease.is_expired(now));
        }
    }

    pub fn len(&self) -> usize {
        self.shards.iter().map(|s| s.lock().len()).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub async fn acquire(&self, user_id: &Uuid, tier: Tier, config: &TierLimitConfig) -> Result<Decision, RateLimitError> {
        let now = Instant::now();
        let shard = self.shard(user_id);

        {
            let mut guard = shard.lock();
            if let Some(lease) = guard.get_mut(user_id)
                && let Some(remaining) = lease.try_consume(now)
            {
                return Ok(Decision::Allowed {
                    remaining,
                    limit: lease.max_limit,
                });
            }
        }

        if let Some(cooldown) = *self.redis_cooldown_until.lock()
            && now < cooldown
        {
            return Ok(self.fallback.check(user_id, tier, config));
        }

        let grant = match self.coordinator.lease_quota(user_id, tier, config).await {
            Ok(grant) => {
                if self.redis_cooldown_until.lock().is_some() {
                    *self.redis_cooldown_until.lock() = None;
                }
                grant
            },
            Err(err) => {
                *self.redis_cooldown_until.lock() = Some(now + Duration::from_secs(5));
                tracing::warn!(%err, "redis rate limiter failed, tripping circuit to autonomous limiter for 5s");
                return Ok(self.fallback.check(user_id, tier, config));
            },
        };

        if grant.granted == 0 {
            return Ok(Decision::Throttled {
                retry_after_secs: grant.retry_after_secs.max(1),
                limit: config.max_tokens,
            });
        }

        let new_expiry = now + self.lease_duration;
        let remaining = grant.granted - 1;

        {
            let mut guard = shard.lock();
            if let Some(lease) = guard.get_mut(user_id) {
                lease.update_lease(tier, remaining, new_expiry, config.max_tokens);
            } else {
                guard.insert(*user_id, LeaseEntry::new(tier, remaining, new_expiry, config.max_tokens));
            }
        }

        Ok(Decision::Allowed {
            remaining,
            limit: config.max_tokens,
        })
    }

    pub fn clear(&self) {
        for shard in self.shards.iter() {
            let mut guard = shard.lock();
            guard.clear();
        }
        self.fallback.clear();
    }
}

#[derive(Clone)]
pub enum RateLimiterEngine {
    Distributed(DistributedLimiter),
    Autonomous(AutonomousLimiter),
    PassThrough,
}

impl RateLimiterEngine {
    pub async fn check(&self, user_id: &Uuid, tier: Tier, config: &TierLimitConfig) -> Result<Decision, RateLimitError> {
        match self {
            Self::Distributed(limiter) => limiter.acquire(user_id, tier, config).await,
            Self::Autonomous(limiter) => Ok(limiter.check(user_id, tier, config)),
            Self::PassThrough => Ok(Decision::Allowed {
                remaining: u64::MAX,
                limit: u64::MAX,
            }),
        }
    }

    pub fn clear(&self) {
        match self {
            Self::Distributed(limiter) => limiter.clear(),
            Self::Autonomous(limiter) => limiter.clear(),
            Self::PassThrough => {},
        }
    }
}
