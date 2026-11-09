use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use axum::{
    body::Body,
    extract::Request,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use tower::{Layer, Service, ServiceExt};
use uuid::Uuid;

use crate::{
    config::{RateLimiterConfig, Tier, TierLimitConfig, TiersConfig},
    layers::rate_limit::{
        layer::RateLimitLayer,
        lease::{AutonomousLimiter, Decision, LeaseEntry, RateLimiterEngine},
        limiter::RateLimiter,
    },
};

fn test_config(enabled: bool) -> Arc<RateLimiterConfig> {
    let tiers = TiersConfig {
        tier1: TierLimitConfig {
            initial_tokens: 1000,
            fill_rate: 1000,
            burst_interval_secs: 60,
            max_tokens: 5000,
            lease_batch_size: 500,
        },
        tier2: TierLimitConfig {
            initial_tokens: 200,
            fill_rate: 200,
            burst_interval_secs: 60,
            max_tokens: 1000,
            lease_batch_size: 100,
        },
        tier3: TierLimitConfig {
            initial_tokens: 50,
            fill_rate: 50,
            burst_interval_secs: 60,
            max_tokens: 250,
            lease_batch_size: 50,
        },
        tier4: TierLimitConfig {
            initial_tokens: 10,
            fill_rate: 10,
            burst_interval_secs: 60,
            max_tokens: 50,
            lease_batch_size: 10,
        },
    };

    let mut role_mapping = HashMap::new();
    role_mapping.insert("admin".to_string(), Tier::Tier1);
    role_mapping.insert("bot".to_string(), Tier::Tier2);
    role_mapping.insert("client".to_string(), Tier::Tier3);
    role_mapping.insert("default".to_string(), Tier::Tier4);

    Arc::new(RateLimiterConfig {
        enabled,
        redis_url: "redis://127.0.0.1:16379".to_string(),
        key_prefix: "rl".to_string(),
        lease_duration_secs: 5,
        batch_window_micros: 100,
        default_tier: Tier::Tier4,
        tiers,
        role_mapping,
    })
}

#[test]
fn test_autonomous_limiter_consumes_down_to_zero_then_throttles() {
    let limiter = AutonomousLimiter::new();
    let user_id = Uuid::new_v4();
    let config = TierLimitConfig {
        initial_tokens: 3,
        fill_rate: 0,
        burst_interval_secs: 60,
        max_tokens: 10,
        lease_batch_size: 1,
    };

    let d1 = limiter.check(&user_id, Tier::Tier4, &config);
    assert_eq!(d1, Decision::Allowed { remaining: 2, limit: 10 });

    let d2 = limiter.check(&user_id, Tier::Tier4, &config);
    assert_eq!(d2, Decision::Allowed { remaining: 1, limit: 10 });

    let d3 = limiter.check(&user_id, Tier::Tier4, &config);
    assert_eq!(d3, Decision::Allowed { remaining: 0, limit: 10 });

    let d4 = limiter.check(&user_id, Tier::Tier4, &config);
    assert_eq!(
        d4,
        Decision::Throttled {
            retry_after_secs: 60,
            limit: 10
        }
    );
}

#[test]
fn test_lease_entry_try_consume() {
    let now = Instant::now();
    let mut entry = LeaseEntry::new(Tier::Tier4, 2, now + Duration::from_secs(10), 10);

    assert_eq!(entry.try_consume(now), Some(1));
    assert_eq!(entry.try_consume(now), Some(0));
    assert_eq!(entry.try_consume(now), None);
}

#[test]
fn test_lease_entry_expiry() {
    let now = Instant::now();
    let mut entry = LeaseEntry::new(Tier::Tier4, 5, now - Duration::from_secs(1), 10);

    assert!(entry.is_expired(now));
    assert_eq!(entry.try_consume(now), None);

    entry.update_lease(Tier::Tier4, 3, now + Duration::from_secs(10), 20);
    assert!(!entry.is_expired(now));
    assert_eq!(entry.try_consume(now), Some(2));
}

#[tokio::test]
async fn test_rate_limiter_disabled_always_allows() {
    let config = test_config(false);
    let limiter = RateLimiter::new(config, RateLimiterEngine::PassThrough);

    let user_id = Uuid::new_v4();
    let decision = limiter.check(&user_id, Tier::Tier4).await.unwrap();

    assert_eq!(
        decision,
        Decision::Allowed {
            remaining: u64::MAX,
            limit: u64::MAX,
        }
    );
}

#[tokio::test]
async fn test_rate_limiter_tier_resolution() {
    let config = test_config(true);
    let limiter = RateLimiter::new(config, RateLimiterEngine::Autonomous(AutonomousLimiter::new()));

    assert_eq!(limiter.resolve_role(Some("admin")), Tier::Tier1);
    assert_eq!(limiter.tier_config(Tier::Tier1).max_tokens, 5000);
    assert_eq!(limiter.tier_config(Tier::Tier1).fill_rate, 1000);

    assert_eq!(limiter.resolve_role(Some("bot")), Tier::Tier2);
    assert_eq!(limiter.tier_config(Tier::Tier2).max_tokens, 1000);

    assert_eq!(limiter.resolve_role(Some("client")), Tier::Tier3);
    assert_eq!(limiter.tier_config(Tier::Tier3).max_tokens, 250);

    assert_eq!(limiter.resolve_role(Some("unknown_role")), Tier::Tier4);
    assert_eq!(limiter.tier_config(Tier::Tier4).max_tokens, 50);

    assert_eq!(limiter.resolve_role(None), Tier::Tier4);
    assert_eq!(limiter.tier_config(Tier::Tier4).max_tokens, 50);
}

#[tokio::test]
async fn test_rate_limit_service_allowed_injects_headers() {
    let config = test_config(true);
    let user_id = Uuid::new_v4();

    let limiter = Arc::new(RateLimiter::new(config, RateLimiterEngine::Autonomous(AutonomousLimiter::new())));
    let layer = RateLimitLayer::new(limiter);

    let inner_service = tower::service_fn(|_req: Request| async { Ok::<Response, std::convert::Infallible>(StatusCode::OK.into_response()) });

    let mut service = layer.layer(inner_service);

    let req = Request::builder()
        .uri("/test")
        .header("x-user-id", user_id.to_string())
        .header("x-user-role", "client")
        .body(Body::empty())
        .unwrap();

    let resp = service.ready().await.unwrap().call(req).await.unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    assert!(resp.headers().contains_key("x-ratelimit-remaining"));
    assert_eq!(resp.headers().get("x-ratelimit-remaining").unwrap(), "49");
    assert_eq!(resp.headers().get("x-ratelimit-limit").unwrap(), "250");
}

#[tokio::test]
async fn test_rate_limit_service_throttled_returns_429() {
    let mut config = (*test_config(true)).clone();
    config.tiers.tier3 = TierLimitConfig {
        initial_tokens: 0,
        fill_rate: 0,
        burst_interval_secs: 15,
        max_tokens: 250,
        lease_batch_size: 1,
    };

    let user_id = Uuid::new_v4();
    let limiter = Arc::new(RateLimiter::new(
        Arc::new(config),
        RateLimiterEngine::Autonomous(AutonomousLimiter::new()),
    ));
    let layer = RateLimitLayer::new(limiter);

    let inner_service = tower::service_fn(|_req: Request| async { Ok::<Response, std::convert::Infallible>(StatusCode::OK.into_response()) });

    let mut service = layer.layer(inner_service);

    let req = Request::builder()
        .uri("/test")
        .header("x-user-id", user_id.to_string())
        .header("x-user-role", "client")
        .body(Body::empty())
        .unwrap();

    let resp = service.ready().await.unwrap().call(req).await.unwrap();

    assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(resp.headers().get("retry-after").unwrap(), "15");
    assert_eq!(resp.headers().get("x-ratelimit-remaining").unwrap(), "0");
    assert_eq!(resp.headers().get("x-ratelimit-limit").unwrap(), "250");
}

#[tokio::test]
async fn test_rate_limit_service_missing_headers_rejected() {
    let config = test_config(true);
    let limiter = Arc::new(RateLimiter::new(config, RateLimiterEngine::Autonomous(AutonomousLimiter::new())));
    let layer = RateLimitLayer::new(limiter);

    let inner_service = tower::service_fn(|_req: Request| async { Ok::<Response, std::convert::Infallible>(StatusCode::OK.into_response()) });

    let mut service = layer.layer(inner_service);

    let req = Request::builder().uri("/test").body(Body::empty()).unwrap();

    let resp = service.ready().await.unwrap().call(req).await.unwrap();

    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_rate_limit_service_disabled_passes_through() {
    let config = test_config(false);
    let user_id = Uuid::new_v4();

    let limiter = Arc::new(RateLimiter::new(config, RateLimiterEngine::PassThrough));
    let layer = RateLimitLayer::new(limiter);

    let inner_service = tower::service_fn(|_req: Request| async { Ok::<Response, std::convert::Infallible>(StatusCode::OK.into_response()) });

    let mut service = layer.layer(inner_service);

    let req = Request::builder()
        .uri("/test")
        .header("x-user-id", user_id.to_string())
        .header("x-user-role", "client")
        .body(Body::empty())
        .unwrap();

    let resp = service.ready().await.unwrap().call(req).await.unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    assert!(!resp.headers().contains_key("retry-after"));
}

#[test]
fn test_autonomous_limiter_sharded_distribution_and_prune() {
    let limiter = AutonomousLimiter::new();
    let config = TierLimitConfig {
        initial_tokens: 10,
        fill_rate: 1,
        burst_interval_secs: 60,
        max_tokens: 50,
        lease_batch_size: 1,
    };

    let now = Instant::now();
    for _ in 0..100 {
        let user_id = Uuid::new_v4();
        let decision = limiter.check(&user_id, Tier::Tier4, &config);
        assert_eq!(decision, Decision::Allowed { remaining: 9, limit: 50 });
    }

    assert_eq!(limiter.len(), 100);

    limiter.prune_stale(now, Duration::from_secs(300));
    assert_eq!(limiter.len(), 100);

    limiter.prune_stale(now + Duration::from_secs(301), Duration::from_secs(300));
    assert_eq!(limiter.len(), 0);
    assert!(limiter.is_empty());
}

#[tokio::test]
async fn test_autonomous_limiter_concurrent_sharded_access() {
    let limiter = Arc::new(AutonomousLimiter::new());
    let config = TierLimitConfig {
        initial_tokens: 100,
        fill_rate: 10,
        burst_interval_secs: 60,
        max_tokens: 100,
        lease_batch_size: 1,
    };

    let mut handles = Vec::new();
    for _ in 0..10 {
        let lim = Arc::clone(&limiter);
        let cfg = config.clone();
        handles.push(tokio::spawn(async move {
            for _ in 0..10 {
                let user_id = Uuid::new_v4();
                let d = lim.check(&user_id, Tier::Tier4, &cfg);
                assert_eq!(d, Decision::Allowed { remaining: 99, limit: 100 });
            }
        }));
    }

    for handle in handles {
        handle.await.unwrap();
    }

    assert_eq!(limiter.len(), 100);
    limiter.clear();
    assert_eq!(limiter.len(), 0);
}
