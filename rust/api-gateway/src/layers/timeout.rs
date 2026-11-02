use std::{pin::Pin, time::Duration};

use axum::{
    extract::Request,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use tower::Service;

pub const DEFAULT_TIMEOUT_SECS: u64 = 30;

#[derive(Clone)]
pub struct TimeoutLayer {
    timeout: Duration,
}

impl TimeoutLayer {
    pub fn new(timeout: Duration) -> Self {
        Self { timeout }
    }
}

impl Default for TimeoutLayer {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(DEFAULT_TIMEOUT_SECS),
        }
    }
}

impl<S> tower::Layer<S> for TimeoutLayer {
    type Service = TimeoutService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        TimeoutService {
            inner,
            timeout: self.timeout,
        }
    }
}

#[derive(Clone)]
pub struct TimeoutService<S> {
    inner: S,
    timeout: Duration,
}

type BoxFut<E> = Pin<Box<dyn std::future::Future<Output = Result<Response, E>> + Send>>;

impl<S> Service<Request> for TimeoutService<S>
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
        let mut inner = self.inner.clone();
        let timeout = self.timeout;

        Box::pin(async move {
            match tokio::time::timeout(timeout, inner.call(req)).await {
                Ok(res) => res,
                Err(_) => Ok(StatusCode::GATEWAY_TIMEOUT.into_response()),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use tower::ServiceExt;

    use super::*;

    #[derive(Clone)]
    struct FastService;

    impl Service<Request> for FastService {
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

    #[derive(Clone)]
    struct SlowService;

    impl Service<Request> for SlowService {
        type Response = Response;
        type Error = std::convert::Infallible;
        type Future = Pin<Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>>;

        fn poll_ready(&mut self, _cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<(), Self::Error>> {
            std::task::Poll::Ready(Ok(()))
        }

        fn call(&mut self, _req: Request) -> Self::Future {
            Box::pin(async move {
                tokio::time::sleep(Duration::from_millis(100)).await;
                Ok(StatusCode::OK.into_response())
            })
        }
    }

    #[tokio::test]
    async fn test_fast_request_passes_timeout() {
        let mut service = TimeoutService {
            inner: FastService,
            timeout: Duration::from_millis(50),
        };
        let req = Request::builder().uri("/test").body(Body::empty()).unwrap();

        let res = service.ready().await.unwrap().call(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_slow_request_triggers_gateway_timeout() {
        let mut service = TimeoutService {
            inner: SlowService,
            timeout: Duration::from_millis(20),
        };
        let req = Request::builder().uri("/test").body(Body::empty()).unwrap();

        let res = service.ready().await.unwrap().call(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::GATEWAY_TIMEOUT);
    }
}
