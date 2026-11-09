use std::{pin::Pin, sync::Arc};

use axum::{
    body::Body,
    extract::Request,
    http::{HeaderName, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use tower::Service;
use uuid::Uuid;

use crate::layers::rate_limit::limiter::{Decision, RateLimiter};

type BoxFut<E> = Pin<Box<dyn std::future::Future<Output = Result<Response, E>> + Send>>;

#[derive(Clone)]
pub struct RateLimitService<S> {
    pub(crate) inner: S,
    pub(crate) limiter: Arc<RateLimiter>,
}

impl<S> Service<Request> for RateLimitService<S>
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
        if !self.limiter.is_enabled() {
            let mut inner = self.inner.clone();
            return Box::pin(async move { inner.call(req).await });
        }

        let user_id = match req
            .headers()
            .get("x-user-id")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| Uuid::parse_str(s).ok())
        {
            Some(id) => id,
            None => return Box::pin(async move { Ok(StatusCode::UNAUTHORIZED.into_response()) }),
        };

        let role = match req.headers().get("x-user-role").and_then(|v| v.to_str().ok()) {
            Some(r) => r,
            None => return Box::pin(async move { Ok(StatusCode::UNAUTHORIZED.into_response()) }),
        };

        let tier = self.limiter.resolve_role(Some(role));
        let limiter = self.limiter.clone();
        let mut inner = self.inner.clone();

        Box::pin(async move {
            let decision = match limiter.check(&user_id, tier).await {
                Ok(d) => d,
                Err(_) => {
                    return Ok((StatusCode::INTERNAL_SERVER_ERROR, "Rate limiter error").into_response());
                },
            };

            match decision {
                Decision::Allowed { remaining, limit } => {
                    let mut resp = inner.call(req).await?;
                    if let Ok(val) = HeaderValue::from_str(&remaining.to_string()) {
                        resp.headers_mut().insert(HeaderName::from_static("x-ratelimit-remaining"), val);
                    }
                    if let Ok(val) = HeaderValue::from_str(&limit.to_string()) {
                        resp.headers_mut().insert(HeaderName::from_static("x-ratelimit-limit"), val);
                    }
                    Ok(resp)
                },
                Decision::Throttled { retry_after_secs, limit } => {
                    let mut resp = Response::builder()
                        .status(StatusCode::TOO_MANY_REQUESTS)
                        .body(Body::from("Too Many Requests"))
                        .unwrap_or_else(|_| StatusCode::TOO_MANY_REQUESTS.into_response());

                    if let Ok(val) = HeaderValue::from_str(&retry_after_secs.to_string()) {
                        resp.headers_mut().insert(HeaderName::from_static("retry-after"), val);
                    }
                    if let Ok(val) = HeaderValue::from_str(&limit.to_string()) {
                        resp.headers_mut().insert(HeaderName::from_static("x-ratelimit-limit"), val);
                    }
                    resp.headers_mut()
                        .insert(HeaderName::from_static("x-ratelimit-remaining"), HeaderValue::from_static("0"));

                    Ok(resp)
                },
            }
        })
    }
}
