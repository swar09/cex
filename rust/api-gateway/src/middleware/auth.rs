use std::{pin::Pin, sync::Arc};

use axum::{
    body::Body,
    extract::Request,
    http::{Error, StatusCode, header::AUTHORIZATION},
    response::{IntoResponse, Response},
};
use jsonwebtoken::{DecodingKey, Validation, decode, decode_header};
use serde::{Deserialize, Serialize};
use tower::Service;
use uuid::Uuid;

use crate::cache::cache::Cache;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Claims {
    pub iss: String,    // issuer (multiple services can assign tokens)
    pub sub: Uuid,      // subject (issued to)
    pub iat: u64,       // time of assignment
    pub nbf: u64,       // time before token is not valid
    pub exp: u64,       // time after which token is not valid
    pub aud: Audience,  // audience (services where token is intended to be used)
    pub role: UserRole, // add by me not mentioned in blog
    pub jti: Uuid,      // token id for jwt blocking purposes
}
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Audience {
    Order,
    Engine,
    Ledger,
    User,
    Kyc,
    Auth,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum UserRole {
    Admin,   // system admin
    Client,  // retail traders
    Partner, // partners traders
    Bot,     // trading bot with api key
}
#[derive(Clone)]
struct AuthState {
    pub cache: Cache,
}

struct AuthLayer<S> {
    inner: S,
    pub state: Arc<AuthState>,
}

impl<S> Service<Request> for AuthLayer<S> {
    type Response = Response;
    type Error = Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>>>>;

    fn poll_ready(&mut self, cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<(), Self::Error>> {
        std::task::Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Request) -> Self::Future {
        let token = match req
            .headers()
            .get(AUTHORIZATION)
            .and_then(|h| h.to_str().ok())
            .and_then(|t| t.strip_prefix("Bearer "))
        {
            Some(t) => t.to_owned(),
            None => {
                return reject(StatusCode::UNAUTHORIZED);
            },
        };

        let jwt_header = match decode_header(&token) {
            Ok(header) => header.to_owned(),
            Err(_) => {
                return reject(StatusCode::UNAUTHORIZED);
            },
        };

        let kid = match jwt_header.kid {
            Some(k) => k,
            None => {
                return reject(StatusCode::UNAUTHORIZED);
            },
        };

        let state = self.state.clone();
        Box::pin(async move {
            let Some(jwk) = state.cache.get_jwk(&kid).await.unwrap() else {
                // cache miss
                // retry
                todo!()
            };
            // error handling

            // algo in the jwk and also in the header ????

            let decoding_key = DecodingKey::from_jwk(&jwk).unwrap();

            let mut validation = Validation::new(jwt_header.alg);
            validation.validate_nbf = true;
            validation.validate_aud = false;

            let claims = match decode::<Claims>(&token, &decoding_key, &validation) {
                Ok(t) => t.claims,
                Err(e) => {
                    return Ok(StatusCode::UNAUTHORIZED.into_response());
                },
            };

            Ok(Response::new(Body::empty()))
        })
    }
}

type BoxFut<E> = Pin<Box<dyn Future<Output = Result<Response, E>> + Send>>;

fn reject<E: Send + 'static>(res: impl IntoResponse) -> BoxFut<E> {
    let res = res.into_response();
    Box::pin(async move { Ok(res) })
}
