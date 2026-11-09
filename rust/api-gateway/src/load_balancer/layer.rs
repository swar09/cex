use std::{collections::HashMap, net::SocketAddr, sync::Arc};

use axum::{
    extract::Request,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use tower::{Layer, Service};

use crate::load_balancer::{backend::Backend, data_plane::LoadBalancer};

#[derive(Clone)]
pub struct SelectedBackend {
    pub addr: SocketAddr,
    pub backend: Arc<Backend>,
}

#[derive(Clone, Default)]
pub struct LoadBalancerRegistry {
    services: Arc<HashMap<String, Arc<LoadBalancer>>>,
}

impl LoadBalancerRegistry {
    pub fn new(services: HashMap<String, Arc<LoadBalancer>>) -> Self {
        Self {
            services: Arc::new(services),
        }
    }

    pub fn get(&self, service_name: &str) -> Option<Arc<LoadBalancer>> {
        self.services.get(service_name).cloned()
    }
}

pub fn resolve_service_name(path: &str) -> Option<&'static str> {
    if path.starts_with("/api/v1/orders") {
        Some("orders")
    } else if path.starts_with("/api/v1/engine") {
        Some("engine")
    } else if path.starts_with("/api/v1/ledger") {
        Some("ledger")
    } else if path.starts_with("/api/v1/user") {
        Some("user")
    } else if path.starts_with("/api/v1/kyc") {
        Some("kyc")
    } else if path.starts_with("/api/v1/auth") {
        Some("auth")
    } else {
        None
    }
}

#[derive(Clone)]
pub struct LoadBalancerLayer {
    registry: LoadBalancerRegistry,
}

impl LoadBalancerLayer {
    pub fn new(registry: LoadBalancerRegistry) -> Self {
        Self { registry }
    }
}

impl<S> Layer<S> for LoadBalancerLayer {
    type Service = LoadBalancerService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        LoadBalancerService {
            inner,
            registry: self.registry.clone(),
        }
    }
}

#[derive(Clone)]
pub struct LoadBalancerService<S> {
    inner: S,
    registry: LoadBalancerRegistry,
}

impl<S> Service<Request> for LoadBalancerService<S>
where
    S: Service<Request, Response = Response> + Clone + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Send + 'static,
{
    type Response = Response;
    type Error = S::Error;
    type Future = std::pin::Pin<Box<dyn std::future::Future<Output = Result<Response, S::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut req: Request) -> Self::Future {
        let service_name = resolve_service_name(req.uri().path());
        let lb = service_name.and_then(|name| self.registry.get(name));

        if let Some(lb) = lb {
            match lb.select() {
                Ok((backend, guard)) => {
                    req.extensions_mut().insert(SelectedBackend {
                        addr: backend.addr,
                        backend: Arc::clone(&backend),
                    });
                    req.extensions_mut().insert(guard);
                },
                Err(_) => {
                    return Box::pin(async move { Ok(StatusCode::SERVICE_UNAVAILABLE.into_response()) });
                },
            }
        }

        let mut inner = self.inner.clone();
        Box::pin(async move { inner.call(req).await })
    }
}
