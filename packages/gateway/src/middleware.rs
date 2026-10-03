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
use jsonwebtoken::{DecodingKey, Validation, decode, decode_header};
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

#[derive(Debug, Clone)]
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

        let jwk = state.cache.get_or_fetch_jwk(&kid).await.map_err(|_| StatusCode::UNAUTHORIZED)?;

        let decoding_key = DecodingKey::from_jwk(&jwk).map_err(|_| StatusCode::UNAUTHORIZED)?;

        let mut validation = Validation::new(jwt_header.alg);
        validation.validate_nbf = true;

        let token_data = decode::<Claims>(token, &decoding_key, &validation).map_err(|_| StatusCode::UNAUTHORIZED)?;

        let claims = token_data.claims;

        if state.cache.is_blacklist(claims.jti).await.map_err(|_| StatusCode::UNAUTHORIZED)? {
            return Err(StatusCode::UNAUTHORIZED);
        }

        Ok(AuthUser {
            user_id: claims.sub,
            jti: claims.jti,
            exp: claims.exp,
        })
    }
}
