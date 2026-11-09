use std::{
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};

use uuid::Uuid;

#[derive(Debug)]
pub struct Backend {
    pub id: Uuid,
    pub addr: SocketAddr,
    pub in_flight: AtomicUsize,
    pub consecutive_transport_failures: AtomicUsize,
    pub is_available: AtomicBool,
}

pub struct BackendGuard {
    backend: Arc<Backend>,
}

impl Clone for BackendGuard {
    fn clone(&self) -> Self {
        self.backend.in_flight.fetch_add(1, Ordering::Relaxed);
        Self {
            backend: Arc::clone(&self.backend),
        }
    }
}

impl BackendGuard {
    pub fn backend(&self) -> &Arc<Backend> {
        &self.backend
    }
}

impl Drop for BackendGuard {
    fn drop(&mut self) {
        self.backend.in_flight.fetch_sub(1, Ordering::Release);
    }
}

impl Backend {
    pub fn new(addr: SocketAddr) -> Self {
        Self {
            id: Uuid::new_v4(),
            addr,
            in_flight: AtomicUsize::new(0),
            consecutive_transport_failures: AtomicUsize::new(0),
            is_available: AtomicBool::new(true),
        }
    }

    pub fn acquire(self: &Arc<Self>) -> BackendGuard {
        self.in_flight.fetch_add(1, Ordering::SeqCst);
        BackendGuard { backend: Arc::clone(self) }
    }

    pub fn mark_transport_failure(&self, threshold: usize) {
        let prev = self.consecutive_transport_failures.fetch_add(1, Ordering::SeqCst);
        if prev + 1 >= threshold {
            self.is_available.store(false, Ordering::Release);
        }
    }

    pub fn mark_transport_success(&self) {
        self.consecutive_transport_failures.store(0, Ordering::Release);
        self.is_available.store(true, Ordering::Release);
    }

    #[inline]
    pub fn is_healthy_and_open(&self) -> bool {
        self.is_available.load(Ordering::Acquire)
    }
}
