use std::pin::Pin;

use axum::{
    extract::Request,
    http::{
        HeaderName,
        header::{CONNECTION, PROXY_AUTHENTICATE, PROXY_AUTHORIZATION, TE, TRAILER, TRANSFER_ENCODING, UPGRADE},
    },
    response::Response,
};
use tower::Service;

static X_USER_ID: HeaderName = HeaderName::from_static("x-user-id");
static X_USER_ROLE: HeaderName = HeaderName::from_static("x-user-role");
static X_TOKEN_ID: HeaderName = HeaderName::from_static("x-token-id");

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
        let headers = req.headers_mut();

        headers.remove(CONNECTION);
        headers.remove(PROXY_AUTHENTICATE);
        headers.remove(PROXY_AUTHORIZATION);
        headers.remove(TE);
        headers.remove(TRAILER);
        headers.remove(TRANSFER_ENCODING);
        headers.remove(UPGRADE);

        headers.remove(&X_USER_ID);
        headers.remove(&X_USER_ROLE);
        headers.remove(&X_TOKEN_ID);

        let mut inner = self.inner.clone();
        Box::pin(async move { inner.call(req).await })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::StatusCode, response::IntoResponse};
    use tower::ServiceExt;

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
            let user_id = req.headers().get("x-user-id").cloned();
            let auth = req.headers().get("authorization").cloned();
            Box::pin(async move {
                let mut res = StatusCode::OK.into_response();
                if let Some(uid) = user_id {
                    res.headers_mut().insert("echo-user-id", uid);
                }
                if let Some(a) = auth {
                    res.headers_mut().insert("echo-auth", a);
                }
                Ok(res)
            })
        }
    }

    #[tokio::test]
    async fn test_strips_malicious_client_headers_leaves_auth() {
        let mut service = HeaderSanitizeService {
            inner: HeaderInspectService,
        };
        let req = Request::builder()
            .uri("/test")
            .header("x-user-id", "attacker-impersonated-id")
            .header("authorization", "Bearer valid-token")
            .header("connection", "close")
            .body(Body::empty())
            .unwrap();

        let res = service.ready().await.unwrap().call(req).await.unwrap();
        assert!(res.headers().get("echo-user-id").is_none());
        assert_eq!(res.headers().get("echo-auth").unwrap(), "Bearer valid-token");
    }
}
