use std::pin::Pin;

use axum::{
    extract::Request,
    http::{StatusCode, header::CONTENT_LENGTH},
    response::{IntoResponse, Response},
};
use tower::Service;

pub const DEFAULT_MAX_CONTENT_LENGTH_BYTES: u64 = 10 * 1024 * 1024;

#[derive(Clone)]
pub struct ContentLengthLimitLayer {
    max_bytes: u64,
}

impl ContentLengthLimitLayer {
    pub fn new(max_bytes: u64) -> Self {
        Self { max_bytes }
    }
}

impl Default for ContentLengthLimitLayer {
    fn default() -> Self {
        Self {
            max_bytes: DEFAULT_MAX_CONTENT_LENGTH_BYTES,
        }
    }
}

impl<S> tower::Layer<S> for ContentLengthLimitLayer {
    type Service = ContentLengthLimitService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        ContentLengthLimitService {
            inner,
            max_bytes: self.max_bytes,
        }
    }
}

#[derive(Clone)]
pub struct ContentLengthLimitService<S> {
    inner: S,
    max_bytes: u64,
}

type BoxFut<E> = Pin<Box<dyn std::future::Future<Output = Result<Response, E>> + Send>>;

impl<S> Service<Request> for ContentLengthLimitService<S>
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

    fn call(&mut self, req: Request) -> Self::Future {
        if let Some(val) = req.headers().get(CONTENT_LENGTH) {
            match val.to_str().ok().and_then(|s| s.parse::<u64>().ok()) {
                Some(len) if len > self.max_bytes => {
                    return Box::pin(async move { Ok(StatusCode::PAYLOAD_TOO_LARGE.into_response()) });
                },
                Some(_) => {},
                None => {
                    return Box::pin(async move { Ok(StatusCode::BAD_REQUEST.into_response()) });
                },
            }
        }

        let mut inner = self.inner.clone();
        Box::pin(async move { inner.call(req).await })
    }
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use tower::ServiceExt;

    use super::*;

    #[derive(Clone)]
    struct OkService;

    impl Service<Request> for OkService {
        type Response = Response;
        type Error = std::convert::Infallible;
        type Future = Pin<Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>>;

        fn poll_ready(&mut self, _cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<(), Self::Error>> {
            std::task::Poll::Ready(Ok(()))
        }

        fn call(&mut self, _req: Request) -> Self::Future {
            Box::pin(async move { Ok(StatusCode::OK.into_response()) })
        }
    }

    #[tokio::test]
    async fn test_payload_under_limit_accepted() {
        let mut service = ContentLengthLimitService {
            inner: OkService,
            max_bytes: 1024,
        };
        let req = Request::builder().uri("/test").header(CONTENT_LENGTH, "500").body(Body::empty()).unwrap();

        let res = service.ready().await.unwrap().call(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_payload_over_limit_rejected() {
        let mut service = ContentLengthLimitService {
            inner: OkService,
            max_bytes: 1024,
        };
        let req = Request::builder()
            .uri("/test")
            .header(CONTENT_LENGTH, "2048")
            .body(Body::empty())
            .unwrap();

        let res = service.ready().await.unwrap().call(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::PAYLOAD_TOO_LARGE);
    }

    #[tokio::test]
    async fn test_malformed_content_length_rejected() {
        let mut service = ContentLengthLimitService {
            inner: OkService,
            max_bytes: 1024,
        };
        let req = Request::builder()
            .uri("/test")
            .header(CONTENT_LENGTH, "not-a-number")
            .body(Body::empty())
            .unwrap();

        let res = service.ready().await.unwrap().call(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    }
}
