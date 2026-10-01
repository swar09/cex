use std::sync::Arc;

use axum::{
    RequestPartsExt,
    extract::FromRequestParts,
    http::{StatusCode, request::Parts},
};
use axum_extra::{
    TypedHeader,
    headers::{Authorization, authorization::Bearer},
};
use jsonwebtoken::{decode_header, jwk};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::AppState;

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserRole {
    Admin,
    Client,
    Partner,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Audience {
    Orders,
    Users,
}

#[derive(Serialize, Deserialize)]
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

pub struct AuthUser {
    pub user_id: Uuid,
    pub jti: Uuid,
    pub exp: u64,
}

impl FromRequestParts<Arc<AppState>> for AuthUser {
    type Rejection = StatusCode;

    async fn from_request_parts(parts: &mut Parts, state: &Arc<AppState>) -> Result<Self, Self::Rejection> {
        let TypedHeader(Authorization(bearer)) = parts
            .extract::<TypedHeader<Authorization<Bearer>>>()
            .await
            .map_err(|_| StatusCode::UNAUTHORIZED)?;

        let token = bearer.token();
        let jwt_header = decode_header(token).map_err(|_| StatusCode::UNAUTHORIZED)?;
        let Some(kid) = jwt_header.kid else {
            return Err(StatusCode::UNAUTHORIZED);
        };

        // let jwk = get_or_fetch() : first get jwk from cache if not then fetch JWKS in
        // cache from well_known json
        // let decoding key from jwk
        // then decode using decoding key
        // if yes then verify the nbf , exp,
        // check jti from auth-service-cache
        // reject if any stage fails
        // handle aud if needed or remove it

        todo!();
        // Ok(AuthUser {
        //     user_id: claims.sub,
        //     jti: claims.jti,
        //     exp: claims.exp,
        // })
    }
}
