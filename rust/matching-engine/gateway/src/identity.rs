use std::str::FromStr;

use axum::{
    extract::FromRequestParts,
    http::{HeaderMap, StatusCode, request::Parts},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const X_USER_ID: &str = "x-user-id";
pub const X_USER_ROLE: &str = "x-user-role";
pub const X_TOKEN_ID: &str = "x-token-id";

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum UserRole {
    Admin,
    Client,
    Partner,
    Bot,
}

impl FromStr for UserRole {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "admin" => Ok(Self::Admin),
            "client" => Ok(Self::Client),
            "partner" => Ok(Self::Partner),
            "bot" => Ok(Self::Bot),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GatewayIdentity {
    pub user_id: Uuid,
    pub role: UserRole,
    pub token_id: Option<Uuid>,
}

impl GatewayIdentity {
    pub fn from_headers(headers: &HeaderMap) -> Option<Self> {
        let header = |name: &str| headers.get(name).and_then(|v| v.to_str().ok());

        let user_id = header(X_USER_ID).and_then(|v| Uuid::parse_str(v).ok())?;
        let role = header(X_USER_ROLE).and_then(|v| v.parse().ok())?;
        let token_id = match header(X_TOKEN_ID) {
            Some(v) => Some(Uuid::parse_str(v).ok()?),
            None => None,
        };

        Some(Self { user_id, role, token_id })
    }
}

impl<S: Send + Sync> FromRequestParts<S> for GatewayIdentity {
    type Rejection = StatusCode;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        Self::from_headers(&parts.headers).ok_or(StatusCode::UNAUTHORIZED)
    }
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderValue;

    use super::*;

    const USER: &str = "00000000-0000-0000-0000-000000000001";
    const TOKEN: &str = "00000000-0000-0000-0000-000000000002";

    fn headers(pairs: &[(&'static str, &str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (k, v) in pairs {
            map.insert(*k, HeaderValue::from_str(v).unwrap());
        }
        map
    }

    #[test]
    fn identity_is_built_from_gateway_headers() {
        let id = GatewayIdentity::from_headers(&headers(&[(X_USER_ID, USER), (X_USER_ROLE, "client"), (X_TOKEN_ID, TOKEN)])).unwrap();
        assert_eq!(id.user_id, Uuid::parse_str(USER).unwrap());
        assert_eq!(id.role, UserRole::Client);
        assert_eq!(id.token_id, Some(Uuid::parse_str(TOKEN).unwrap()));
    }

    #[test]
    fn token_id_is_optional() {
        let id = GatewayIdentity::from_headers(&headers(&[(X_USER_ID, USER), (X_USER_ROLE, "bot")])).unwrap();
        assert_eq!(id.role, UserRole::Bot);
        assert_eq!(id.token_id, None);
    }

    #[test]
    fn missing_user_id_is_rejected() {
        assert!(GatewayIdentity::from_headers(&headers(&[(X_USER_ROLE, "admin")])).is_none());
    }

    #[test]
    fn missing_role_is_rejected() {
        assert!(GatewayIdentity::from_headers(&headers(&[(X_USER_ID, USER)])).is_none());
    }

    #[test]
    fn malformed_values_are_rejected() {
        assert!(GatewayIdentity::from_headers(&headers(&[(X_USER_ID, "42"), (X_USER_ROLE, "client")])).is_none());
        assert!(GatewayIdentity::from_headers(&headers(&[(X_USER_ID, USER), (X_USER_ROLE, "superadmin")])).is_none());
        assert!(GatewayIdentity::from_headers(&headers(&[(X_USER_ID, USER), (X_USER_ROLE, "client"), (X_TOKEN_ID, "x")])).is_none());
    }

    #[test]
    fn role_matches_api_gateway_serialization() {
        for role in [UserRole::Admin, UserRole::Client, UserRole::Partner, UserRole::Bot] {
            let s = serde_json::to_string(&role).unwrap();
            assert_eq!(s.trim_matches('"').parse::<UserRole>(), Ok(role));
        }
    }
}
