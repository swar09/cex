use std::pin::Pin;

use axum::{extract::Request, http::HeaderName, response::Response};
use tower::Service;

#[inline]
fn is_whitelisted_header(name: &HeaderName) -> bool {
    matches!(
        name.as_str(),
        "authorization" | "content-type" | "content-length" | "accept" | "accept-encoding" | "user-agent" | "host" | "origin" | "x-request-id"
    )
}

#[derive(Clone, Default)]
pub struct HeaderSanitizeLayer;

impl<S> tower::Layer<S> for HeaderSanitizeLayer {
    type Service = HeaderSanitizeService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        HeaderSanitizeService { inner }
    }
}

#[derive(Clone)]
pub struct HeaderSanitizeService<S> {
    inner: S,
}

type BoxFut<E> = Pin<Box<dyn std::future::Future<Output = Result<Response, E>> + Send>>;

impl<S> Service<Request> for HeaderSanitizeService<S>
where
    S: Service<Request, Response = Response> + Clone + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Send + 'static,
{
    type Response = Response;
    type Error = S::Error;
    type Future = BoxFut<Self::Error>;

    fn poll_ready(&mut self, cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut req: Request) -> Self::Future {
        let disallowed: Vec<HeaderName> = req.headers().keys().filter(|name| !is_whitelisted_header(name)).cloned().collect();

        for name in disallowed {
            req.headers_mut().remove(name);
        }

        let mut inner = self.inner.clone();
        Box::pin(async move { inner.call(req).await })
    }
}

#[cfg(test)]
mod tests {
    use axum::{body::Body, http::StatusCode, response::IntoResponse};
    use tower::ServiceExt;

    use super::*;

    #[derive(Clone)]
    struct HeaderInspectService;

    impl Service<Request> for HeaderInspectService {
        type Response = Response;
        type Error = std::convert::Infallible;
        type Future = Pin<Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>>;

        fn poll_ready(&mut self, _cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<(), Self::Error>> {
            std::task::Poll::Ready(Ok(()))
        }

        fn call(&mut self, req: Request) -> Self::Future {
            let headers = req.headers().clone();
            Box::pin(async move {
                let mut res = StatusCode::OK.into_response();
                for (name, val) in &headers {
                    res.headers_mut().insert(name.clone(), val.clone());
                }
                Ok(res)
            })
        }
    }

    #[tokio::test]
    async fn test_drops_caching_and_conditional_headers() {
        let mut service = HeaderSanitizeService { inner: HeaderInspectService };
        let req = Request::builder()
            .uri("/test")
            .header("if-match", "\"xyzzy\"")
            .header("if-none-match", "W/\"xyzzy\"")
            .header("if-modified-since", "Sat, 29 Oct 2026 19:43:31 GMT")
            .header("if-unmodified-since", "Sat, 29 Oct 2026 19:43:31 GMT")
            .header("if-range", "\"xyzzy\"")
            .header("cache-control", "no-cache, no-store")
            .header("pragma", "no-cache")
            .header("content-type", "application/json")
            .body(Body::empty())
            .unwrap();

        let res = service.ready().await.unwrap().call(req).await.unwrap();
        assert!(res.headers().get("if-match").is_none());
        assert!(res.headers().get("if-none-match").is_none());
        assert!(res.headers().get("if-modified-since").is_none());
        assert!(res.headers().get("if-unmodified-since").is_none());
        assert!(res.headers().get("if-range").is_none());
        assert!(res.headers().get("cache-control").is_none());
        assert!(res.headers().get("pragma").is_none());
        assert_eq!(res.headers().get("content-type").unwrap(), "application/json");
    }

    #[tokio::test]
    async fn test_drops_hop_by_hop_and_proxy_headers() {
        let mut service = HeaderSanitizeService { inner: HeaderInspectService };
        let req = Request::builder()
            .uri("/test")
            .header("connection", "keep-alive")
            .header("proxy-connection", "keep-alive")
            .header("proxy-authorization", "Basic credentials")
            .header("proxy-authenticate", "Basic")
            .header("te", "trailers")
            .header("trailer", "X-Custom")
            .header("transfer-encoding", "chunked")
            .header("upgrade", "websocket")
            .header("keep-alive", "timeout=5")
            .header("x-forwarded-for", "203.0.113.195, 70.41.3.18")
            .header("x-forwarded-proto", "https")
            .header("x-forwarded-host", "internal.corp")
            .header("x-forwarded-port", "443")
            .header("x-real-ip", "10.0.0.1")
            .header("forwarded", "for=192.0.2.60;proto=http")
            .header("via", "1.1 varnish")
            .header("user-agent", "Mozilla/5.0")
            .body(Body::empty())
            .unwrap();

        let res = service.ready().await.unwrap().call(req).await.unwrap();
        assert!(res.headers().get("connection").is_none());
        assert!(res.headers().get("proxy-connection").is_none());
        assert!(res.headers().get("proxy-authorization").is_none());
        assert!(res.headers().get("proxy-authenticate").is_none());
        assert!(res.headers().get("te").is_none());
        assert!(res.headers().get("trailer").is_none());
        assert!(res.headers().get("transfer-encoding").is_none());
        assert!(res.headers().get("upgrade").is_none());
        assert!(res.headers().get("keep-alive").is_none());
        assert!(res.headers().get("x-forwarded-for").is_none());
        assert!(res.headers().get("x-forwarded-proto").is_none());
        assert!(res.headers().get("x-forwarded-host").is_none());
        assert!(res.headers().get("x-forwarded-port").is_none());
        assert!(res.headers().get("x-real-ip").is_none());
        assert!(res.headers().get("forwarded").is_none());
        assert!(res.headers().get("via").is_none());
        assert_eq!(res.headers().get("user-agent").unwrap(), "Mozilla/5.0");
    }

    #[tokio::test]
    async fn test_drops_spoofed_internal_identity_headers() {
        let mut service = HeaderSanitizeService { inner: HeaderInspectService };
        let req = Request::builder()
            .uri("/test")
            .header("x-user-id", "00000000-0000-0000-0000-000000000001")
            .header("x-user-role", "SuperAdmin")
            .header("x-token-id", "00000000-0000-0000-0000-000000000002")
            .header("x-admin", "true")
            .header("x-internal-secret", "secret-key")
            .header("x-consumer-id", "99")
            .header("x-tenant-id", "corp-tenant")
            .header("accept", "application/json")
            .body(Body::empty())
            .unwrap();

        let res = service.ready().await.unwrap().call(req).await.unwrap();
        assert!(res.headers().get("x-user-id").is_none());
        assert!(res.headers().get("x-user-role").is_none());
        assert!(res.headers().get("x-token-id").is_none());
        assert!(res.headers().get("x-admin").is_none());
        assert!(res.headers().get("x-internal-secret").is_none());
        assert!(res.headers().get("x-consumer-id").is_none());
        assert!(res.headers().get("x-tenant-id").is_none());
        assert_eq!(res.headers().get("accept").unwrap(), "application/json");
    }

    #[tokio::test]
    async fn test_drops_browser_and_external_tracing_headers() {
        let mut service = HeaderSanitizeService { inner: HeaderInspectService };
        let req = Request::builder()
            .uri("/test")
            .header("cookie", "session_id=attacker_session; tracking=true")
            .header("set-cookie", "injected_session=1")
            .header("referer", "https://phishing.example.com")
            .header("sec-ch-ua", "\"Chromium\";v=\"120\"")
            .header("sec-fetch-dest", "empty")
            .header("sec-fetch-mode", "cors")
            .header("sec-fetch-site", "same-origin")
            .header("traceparent", "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01")
            .header("tracestate", "congo=t61rcWkgMzE")
            .header("x-b3-traceid", "463ac35c9f6413ad48485a3953bb6124")
            .header("x-b3-spanid", "a2fb4a1d1a96d312")
            .header("host", "api.cex.internal")
            .body(Body::empty())
            .unwrap();

        let res = service.ready().await.unwrap().call(req).await.unwrap();
        assert!(res.headers().get("cookie").is_none());
        assert!(res.headers().get("set-cookie").is_none());
        assert!(res.headers().get("referer").is_none());
        assert!(res.headers().get("sec-ch-ua").is_none());
        assert!(res.headers().get("sec-fetch-dest").is_none());
        assert!(res.headers().get("sec-fetch-mode").is_none());
        assert!(res.headers().get("sec-fetch-site").is_none());
        assert!(res.headers().get("traceparent").is_none());
        assert!(res.headers().get("tracestate").is_none());
        assert!(res.headers().get("x-b3-traceid").is_none());
        assert!(res.headers().get("x-b3-spanid").is_none());
        assert_eq!(res.headers().get("host").unwrap(), "api.cex.internal");
    }

    #[tokio::test]
    async fn test_retains_all_whitelisted_headers() {
        let mut service = HeaderSanitizeService { inner: HeaderInspectService };
        let req = Request::builder()
            .uri("/test")
            .header("authorization", "Bearer valid-token")
            .header("content-type", "application/json")
            .header("content-length", "128")
            .header("accept", "application/json, text/plain")
            .header("accept-encoding", "gzip, deflate, br")
            .header("user-agent", "cex-mobile-client/1.0.0")
            .header("host", "api.cex.com")
            .header("origin", "https://cex.com")
            .header("x-request-id", "018f45a0-d790-7cb8-9b80-60b7713d2a58")
            .body(Body::empty())
            .unwrap();

        let res = service.ready().await.unwrap().call(req).await.unwrap();
        assert_eq!(res.headers().len(), 9);
        assert_eq!(res.headers().get("authorization").unwrap(), "Bearer valid-token");
        assert_eq!(res.headers().get("content-type").unwrap(), "application/json");
        assert_eq!(res.headers().get("content-length").unwrap(), "128");
        assert_eq!(res.headers().get("accept").unwrap(), "application/json, text/plain");
        assert_eq!(res.headers().get("accept-encoding").unwrap(), "gzip, deflate, br");
        assert_eq!(res.headers().get("user-agent").unwrap(), "cex-mobile-client/1.0.0");
        assert_eq!(res.headers().get("host").unwrap(), "api.cex.com");
        assert_eq!(res.headers().get("origin").unwrap(), "https://cex.com");
        assert_eq!(res.headers().get("x-request-id").unwrap(), "018f45a0-d790-7cb8-9b80-60b7713d2a58");
    }
}
