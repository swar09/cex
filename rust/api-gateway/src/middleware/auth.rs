use std::pin::Pin;

use axum::{
    extract::Request,
    http::{Error, StatusCode, header::AUTHORIZATION},
    response::{IntoResponse, Response},
};
use jsonwebtoken::decode_header;
use serde::{Deserialize, Serialize};
use tower::Service;
use uuid::Uuid;

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

struct AuthLayer {
    // inner :
}

impl Service<Request> for AuthLayer {
    type Response = Response;
    type Error = Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>>>>;

    fn poll_ready(&mut self, cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<(), Self::Error>> {
        std::task::Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Request) -> Self::Future {
        let Some(jwt_header) = req
            .headers()
            .get(AUTHORIZATION)
            .and_then(|h| h.to_str().ok())
            .and_then(|t| t.strip_prefix("Bearer "))
            .and_then(|jwt| decode_header(jwt).ok())
        else {
            return reject(StatusCode::UNAUTHORIZED);
        };

        let Some(kid) = jwt_header.kid else {
            return reject(StatusCode::UNAUTHORIZED);
        };

        // let jwk = cache.get_jwk(kid) ? return 500;
        // let decoding_key = DecodingKey::from_jwk(&jwk) ? return 500;
        // let mut validation = Validation::new(jwt_header.alg);
        // validation.validate_nbf = true;
        // validation.validate_aud = false;

        // let token_data = decode::<Claims>(&token, &decoding_key, &validation) ? return Unauth
        // let claims = token_data.claims;

        // if state.cache.is_blacklist(claims.jti).await ? return 500 {
        //     return reject(StatusCode::UNAUTHORIZED);
        // }

        
        todo!()
    }
}

type BoxFut<E> = Pin<Box<dyn Future<Output = Result<Response, E>> + Send>>;

fn reject<E: Send + 'static>(res: impl IntoResponse) -> BoxFut<E> {
    let res = res.into_response();
    Box::pin(async move { Ok(res) })
}
