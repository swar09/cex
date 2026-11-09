use std::{
    net::SocketAddr,
    sync::{Arc, atomic::Ordering},
};

use crate::load_balancer::{
    backend::Backend,
    config::{AlgorithmType, CircuitBreakerConfig, HealthCheckConfig, LoadBalancerConfig},
    data_plane::LoadBalancer,
    error::LoadBalancerError,
    strategy::{LoadBalancingStrategy, PowerOfTwoChoices, RoundRobin},
};

fn test_addr(port: u16) -> SocketAddr {
    format!("127.0.0.1:{}", port).parse().unwrap()
}

#[test]
fn test_p2c_selects_least_loaded() {
    let b1 = Arc::new(Backend::new(test_addr(8001)));
    let b2 = Arc::new(Backend::new(test_addr(8002)));
    let b3 = Arc::new(Backend::new(test_addr(8003)));
    let b4 = Arc::new(Backend::new(test_addr(8004)));

    b1.in_flight.store(10, Ordering::SeqCst);
    b2.in_flight.store(2, Ordering::SeqCst);
    b3.in_flight.store(8, Ordering::SeqCst);
    b4.in_flight.store(1, Ordering::SeqCst);

    let backends = vec![b1, b2, b3, b4];
    let strategy = PowerOfTwoChoices;

    for _ in 0..50 {
        let selected = strategy.select("test-service", &backends).unwrap();
        assert!(selected.in_flight.load(Ordering::Relaxed) <= 8);
    }
}

#[test]
fn test_p2c_single_backend_edge_case() {
    let b1 = Arc::new(Backend::new(test_addr(8001)));
    let backends = vec![b1];
    let strategy = PowerOfTwoChoices;

    let selected = strategy.select("test-service", &backends).unwrap();
    assert_eq!(selected.addr, test_addr(8001));
}

#[test]
fn test_p2c_all_unhealthy_returns_error() {
    let b1 = Arc::new(Backend::new(test_addr(8001)));
    let b2 = Arc::new(Backend::new(test_addr(8002)));
    b1.is_available.store(false, Ordering::SeqCst);
    b2.is_available.store(false, Ordering::SeqCst);

    let backends = vec![b1, b2];
    let strategy = PowerOfTwoChoices;

    let err = strategy.select("test-service", &backends).unwrap_err();
    assert!(matches!(err, LoadBalancerError::NoHealthyBackends { .. }));
}

#[test]
fn test_round_robin_strict_sequence() {
    let b1 = Arc::new(Backend::new(test_addr(8001)));
    let b2 = Arc::new(Backend::new(test_addr(8002)));
    let b3 = Arc::new(Backend::new(test_addr(8003)));
    let backends = vec![b1, b2, b3];
    let strategy = RoundRobin::default();

    let s1 = strategy.select("test-service", &backends).unwrap();
    let s2 = strategy.select("test-service", &backends).unwrap();
    let s3 = strategy.select("test-service", &backends).unwrap();
    let s4 = strategy.select("test-service", &backends).unwrap();

    assert_eq!(s1.addr, test_addr(8001));
    assert_eq!(s2.addr, test_addr(8002));
    assert_eq!(s3.addr, test_addr(8003));
    assert_eq!(s4.addr, test_addr(8001));
}

#[test]
fn test_guard_increments_on_acquire_and_decrements_on_drop() {
    let backend = Arc::new(Backend::new(test_addr(8001)));
    assert_eq!(backend.in_flight.load(Ordering::SeqCst), 0);

    {
        let guard = backend.acquire();
        assert_eq!(guard.backend().in_flight.load(Ordering::SeqCst), 1);
    }

    assert_eq!(backend.in_flight.load(Ordering::SeqCst), 0);
}

#[test]
fn test_guard_decrements_on_panic_unwind() {
    let backend = Arc::new(Backend::new(test_addr(8001)));
    let backend_clone = Arc::clone(&backend);

    let result = std::panic::catch_unwind(move || {
        let _guard = backend_clone.acquire();
        assert_eq!(backend_clone.in_flight.load(Ordering::SeqCst), 1);
        panic!("simulated task panic");
    });

    assert!(result.is_err());
    assert_eq!(backend.in_flight.load(Ordering::SeqCst), 0);
}

#[test]
fn test_transport_failure_trips_circuit_at_threshold() {
    let backend = Backend::new(test_addr(8001));
    assert!(backend.is_healthy_and_open());

    backend.mark_transport_failure(3);
    assert!(backend.is_healthy_and_open());

    backend.mark_transport_failure(3);
    assert!(backend.is_healthy_and_open());

    backend.mark_transport_failure(3);
    assert!(!backend.is_healthy_and_open());
}

#[test]
fn test_transport_success_resets_consecutive_failure_counter() {
    let backend = Backend::new(test_addr(8001));
    backend.mark_transport_failure(3);
    backend.mark_transport_failure(3);
    backend.mark_transport_success();

    assert!(backend.is_healthy_and_open());
    assert_eq!(backend.consecutive_transport_failures.load(Ordering::SeqCst), 0);

    backend.mark_transport_failure(3);
    assert!(backend.is_healthy_and_open());
}

#[test]
fn test_atomic_swap_preserves_inflight_requests_on_old_slice() {
    let b_a = test_addr(8001);
    let b_b = test_addr(8002);

    let config = LoadBalancerConfig {
        service_name: "orders".to_string(),
        algorithm: AlgorithmType::RoundRobin,
        health_check: HealthCheckConfig::default(),
        circuit_breaker: CircuitBreakerConfig::default(),
        initial_endpoints: vec![b_a],
    };

    let lb = LoadBalancer::new(config).unwrap();

    let (selected_a, guard_a) = lb.select().unwrap();
    assert_eq!(selected_a.addr, b_a);
    assert_eq!(selected_a.in_flight.load(Ordering::SeqCst), 1);

    let new_backend_b = Arc::new(Backend::new(b_b));
    lb.swap_backends(vec![Arc::clone(&new_backend_b)]);

    let (selected_b, guard_b) = lb.select().unwrap();
    assert_eq!(selected_b.addr, b_b);
    assert_eq!(new_backend_b.in_flight.load(Ordering::SeqCst), 1);

    assert_eq!(selected_a.in_flight.load(Ordering::SeqCst), 1);
    drop(guard_a);
    assert_eq!(selected_a.in_flight.load(Ordering::SeqCst), 0);

    drop(guard_b);
    assert_eq!(new_backend_b.in_flight.load(Ordering::SeqCst), 0);
}
