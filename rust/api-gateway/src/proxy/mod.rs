use std::{sync::Arc, time::Duration};

use axum::{
    body::Body,
    extract::{Request, State},
    http::{HeaderValue, StatusCode, Uri, header::HOST},
    response::{IntoResponse, Response},
};
use hyper_util::{
    client::legacy::{Client, connect::HttpConnector},
    rt::TokioExecutor,
};

use crate::config::ServicesConfig;

pub type HyperPool = Client<HttpConnector, Body>;

#[derive(Clone)]
pub struct ProxyClient {
    pub client: HyperPool,
}

impl Default for ProxyClient {
    fn default() -> Self {
        Self::new()
    }
}

impl ProxyClient {
    pub fn new() -> Self {
        let mut connector = HttpConnector::new();
        connector.set_nodelay(true);
        connector.set_keepalive(Some(Duration::from_secs(60)));

        let client = Client::builder(TokioExecutor::new())
            .pool_idle_timeout(Duration::from_secs(90))
            .pool_max_idle_per_host(32)
            .http2_keep_alive_interval(Some(Duration::from_secs(30)))
            .build(connector);

        Self { client }
    }

    pub fn from_client(client: HyperPool) -> Self {
        Self { client }
    }
}

pub trait ProxyRouteProvider: Send + Sync {
    fn services(&self) -> &ServicesConfig;
    fn proxy_client(&self) -> &ProxyClient;
}

pub async fn reverse_proxy_handler<T>(State(state): State<Arc<T>>, mut req: Request) -> Response
where
    T: ProxyRouteProvider + 'static,
{
    let selected = req.extensions().get::<crate::load_balancer::SelectedBackend>().cloned();

    let path = req.uri().path();
    let upstream_base = if let Some(ref sel) = selected {
        format!("http://{}", sel.addr)
    } else {
        match state.services().resolve_upstream(path) {
            Some(url) => url.to_string(),
            None => return StatusCode::NOT_FOUND.into_response(),
        }
    };

    let path_and_query = req.uri().path_and_query().map(|pq| pq.as_str()).unwrap_or(path);

    let target_uri_str = format!("{}{}", upstream_base, path_and_query);
    let target_uri = match target_uri_str.parse::<Uri>() {
        Ok(uri) => uri,
        Err(_) => return StatusCode::BAD_GATEWAY.into_response(),
    };

    *req.uri_mut() = target_uri;

    if let Some(host_val) = req.uri().authority().and_then(|a| HeaderValue::from_str(a.as_str()).ok()) {
        req.headers_mut().insert(HOST, host_val);
    }

    match state.proxy_client().client.request(req).await {
        Ok(upstream_res) => {
            if let Some(ref sel) = selected {
                if upstream_res.status().is_server_error() {
                    sel.backend.mark_transport_failure(3);
                } else {
                    sel.backend.mark_transport_success();
                }
            }
            let (parts, upstream_body) = upstream_res.into_parts();
            Response::from_parts(parts, Body::new(upstream_body))
        },
        Err(err) => {
            if let Some(ref sel) = selected {
                sel.backend.mark_transport_failure(3);
            }
            tracing::error!(%err, "upstream service unreachable");
            StatusCode::BAD_GATEWAY.into_response()
        },
    }
}
