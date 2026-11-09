use std::{net::SocketAddr, sync::Arc, time::Duration};

use tokio::time::sleep;

use crate::load_balancer::{config::HealthCheckConfig, data_plane::LoadBalancer};

pub struct ControlPlane {
    lb: Arc<LoadBalancer>,
    config: HealthCheckConfig,
}

impl ControlPlane {
    pub fn new(lb: Arc<LoadBalancer>, config: HealthCheckConfig) -> Self {
        Self { lb, config }
    }

    pub async fn probe_endpoint(addr: SocketAddr, path: &str, timeout_ms: u64) -> bool {
        let target = format!("http://{}{}", addr, path);
        let client = match reqwest::Client::builder().timeout(Duration::from_millis(timeout_ms)).build() {
            Ok(c) => c,
            Err(_) => return false,
        };

        match client.get(&target).send().await {
            Ok(resp) => resp.status().is_success(),
            Err(_) => false,
        }
    }

    pub fn spawn_health_checker(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let interval = Duration::from_millis(self.config.interval_ms);
            loop {
                sleep(interval).await;
                self.check_once().await;
            }
        })
    }

    pub async fn check_once(&self) {
        let backends = self.lb.backends();
        for backend in backends.iter() {
            let success = Self::probe_endpoint(backend.addr, &self.config.path, self.config.timeout_ms).await;
            if success {
                backend.mark_transport_success();
            } else {
                backend.mark_transport_failure(self.config.consecutive_failure_threshold);
            }
        }
    }
}
