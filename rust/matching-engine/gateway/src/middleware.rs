use std::sync::Arc;

use axum::{
    extract::FromRequestParts,
    http::{StatusCode, request::Parts},
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

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = StatusCode;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let token = if let Some(auth) = parts.headers.get(axum::http::header::AUTHORIZATION).and_then(|h| h.to_str().ok()) {
            if let Some(token) = auth.strip_prefix("Bearer ") {
                token.to_string()
            } else {
                return Err(StatusCode::UNAUTHORIZED);
            }
        } else if let Some(query) = parts.uri.query() {
            query
                .split('&')
                .find_map(|pair| {
                    let (k, v) = pair.split_once('=')?;
                    if k == "token" { Some(v.to_string()) } else { None }
                })
                .ok_or(StatusCode::UNAUTHORIZED)?
        } else {
            return Err(StatusCode::UNAUTHORIZED);
        };

        let jwt_header = decode_header(&token).map_err(|_| StatusCode::UNAUTHORIZED)?;
        let Some(kid) = jwt_header.kid else {
            return Err(StatusCode::UNAUTHORIZED);
        };

        let jwk = state.cache.get_or_fetch_jwk(&kid).await.map_err(|_| StatusCode::UNAUTHORIZED)?;

        let decoding_key = DecodingKey::from_jwk(&jwk).map_err(|_| StatusCode::UNAUTHORIZED)?;

        let mut validation = Validation::new(jwt_header.alg);
        validation.validate_nbf = true;
        validation.validate_aud = false;

        let token_data = decode::<Claims>(&token, &decoding_key, &validation).map_err(|_| StatusCode::UNAUTHORIZED)?;

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

impl FromRequestParts<Arc<AppState>> for AuthUser {
    type Rejection = StatusCode;

    async fn from_request_parts(parts: &mut Parts, state: &Arc<AppState>) -> Result<Self, Self::Rejection> {
        Self::from_request_parts(parts, state.as_ref()).await
    }
}
