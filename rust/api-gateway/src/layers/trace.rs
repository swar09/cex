use std::pin::Pin;

use axum::{
    extract::Request,
    http::{HeaderName, HeaderValue},
    response::Response,
};
use tower::Service;
use tracing::{Instrument, info_span};
use uuid::Uuid;

static X_REQUEST_ID: HeaderName = HeaderName::from_static("x-request-id");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RequestId(pub Uuid);

#[derive(Clone, Default)]
pub struct TraceLayer;

impl<S> tower::Layer<S> for TraceLayer {
    type Service = TraceService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        TraceService { inner }
    }
}

#[derive(Clone)]
pub struct TraceService<S> {
    inner: S,
}

type BoxFut<E> = Pin<Box<dyn std::future::Future<Output = Result<Response, E>> + Send>>;

impl<S> Service<Request> for TraceService<S>
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
        let request_id = Uuid::now_v7();

        if let Ok(val) = HeaderValue::from_str(&request_id.to_string()) {
            req.headers_mut().insert(X_REQUEST_ID.clone(), val);
        }

        req.extensions_mut().insert(RequestId(request_id));

        let span = info_span!(
            "http_request",
            %request_id,
            method = %req.method(),
            uri = %req.uri(),
        );

        let mut inner = self.inner.clone();
        Box::pin(
            async move {
                let mut response = inner.call(req).await?;
                if let Ok(val) = HeaderValue::from_str(&request_id.to_string()) {
                    response.headers_mut().insert(X_REQUEST_ID.clone(), val);
                }
                Ok(response)
            }
            .instrument(span),
        )
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use axum::{body::Body, http::StatusCode};
    use tower::ServiceExt;

    use super::*;

    #[derive(Clone)]
    struct EchoService;

    impl Service<Request> for EchoService {
        type Response = Response;
        type Error = std::convert::Infallible;
        type Future = Pin<Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>>;

        fn poll_ready(&mut self, _cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<(), Self::Error>> {
            std::task::Poll::Ready(Ok(()))
        }

        fn call(&mut self, req: Request) -> Self::Future {
            let id = req.extensions().get::<RequestId>().copied();
            Box::pin(async move {
                let body = id.map(|i| i.0.to_string()).unwrap_or_default();
                Ok(Response::builder().status(StatusCode::OK).body(Body::from(body)).unwrap())
            })
        }
    }

    #[tokio::test]
    async fn test_generates_request_id_v7_when_missing() {
        let mut service = TraceService { inner: EchoService };
        let req = Request::builder().uri("/test").body(Body::empty()).unwrap();

        let res = service.ready().await.unwrap().call(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        let header = res.headers().get("x-request-id").expect("header must exist");
        let parsed = Uuid::from_str(header.to_str().unwrap()).unwrap();
        assert_eq!(parsed.get_version_num(), 7);
    }

    #[tokio::test]
    async fn test_overwrites_client_supplied_request_id() {
        let incoming_id = Uuid::new_v4();
        let mut service = TraceService { inner: EchoService };
        let req = Request::builder()
            .uri("/test")
            .header("x-request-id", incoming_id.to_string())
            .body(Body::empty())
            .unwrap();

        let res = service.ready().await.unwrap().call(req).await.unwrap();
        let header = res.headers().get("x-request-id").unwrap().to_str().unwrap();
        let parsed = Uuid::from_str(header).unwrap();
        assert_ne!(parsed, incoming_id);
        assert_eq!(parsed.get_version_num(), 7);
    }
}
