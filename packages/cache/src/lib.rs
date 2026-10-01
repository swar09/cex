pub mod error;

use std::time::Duration;

use domain::{ModifyOrder, NewOrder, Order, OrderId};
pub use error::CacheError;
use jsonwebtoken::jwk::Jwk;
use redis::{
    AsyncCommands, Client, SetOptions,
    aio::{ConnectionManager, ConnectionManagerConfig},
};
use uuid::Uuid;

#[derive(Clone)]
pub struct Cache {
    pub connection_manager: ConnectionManager, // cheap clone its Arc<Internals>
}

impl Cache {
    pub async fn new(redis_addr: &str) -> Result<Self, CacheError> {
        let config = ConnectionManagerConfig::new()
            .set_connection_timeout(Some(Duration::from_secs(3)))
            .set_response_timeout(Some(Duration::from_secs(2)));
        let client = Client::open(redis_addr)?;
        let connection_manager = ConnectionManager::new_with_config(client, config).await?;

        Ok(Cache { connection_manager })
    }

    pub async fn new_with_config(redis_addr: &str, config: ConnectionManagerConfig) -> Result<Self, CacheError> {
        let client = Client::open(redis_addr)?;
        let connection_manager = ConnectionManager::new_with_config(client, config).await?;

        Ok(Cache { connection_manager })
    }

    pub async fn blacklist(&self, jti: Uuid, exp_rsec: u64) -> Result<bool, CacheError> {
        let mut conn = self.connection_manager.clone();
        let options = SetOptions::default().with_expiration(redis::SetExpiry::EX(exp_rsec));
        let key = format!("blacklist:{jti}");
        let value = format!("{jti}");
        let result: Option<String> = conn.set_options(key, value, options).await?;

        Ok(result.as_deref() == Some("OK"))
    }

    pub async fn is_blacklist(&self, jti: Uuid) -> Result<bool, CacheError> {
        let mut conn = self.connection_manager.clone();
        let key = format!("blacklist:{jti}");
        let result: bool = conn.exists(key).await?;

        Ok(result)
    }

    pub async fn get_jwk(&self, kid: &str) -> Result<Option<Jwk>, CacheError> {
        let mut conn = self.connection_manager.clone();
        let key = format!("jwks:{kid}");
        let data: Option<String> = conn.get(key).await?;
        match data {
            Some(json_str) => {
                let jwk: Jwk = serde_json::from_str(&json_str)?;
                Ok(Some(jwk))
            },
            None => Ok(None),
        }
    }

    pub async fn set_jwk(&self, kid: &str, jwk: &Jwk, exp_sec: Option<u64>) -> Result<(), CacheError> {
        let mut conn = self.connection_manager.clone();
        let key = format!("jwks:{kid}");
        let json_str = serde_json::to_string(jwk)?;
        if let Some(ttl) = exp_sec {
            let options = SetOptions::default().with_expiration(redis::SetExpiry::EX(ttl));
            let _: Option<String> = conn.set_options(key, json_str, options).await?;
        } else {
            let _: () = conn.set(key, json_str).await?;
        }
        Ok(())
    }

    pub async fn get_or_fetch_jwk<F, Fut>(&self, kid: &str, fetch: F, exp_sec: Option<u64>) -> Result<Jwk, CacheError>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<Jwk, CacheError>>,
    {
        if let Some(cached) = self.get_jwk(kid).await? {
            return Ok(cached);
        }

        let fetched = fetch().await?;
        self.set_jwk(kid, &fetched, exp_sec).await?;
        Ok(fetched)
    }

    pub async fn add_order(&self, order: &Order, exp_sec: u64) -> Result<bool, CacheError> {
        let mut conn = self.connection_manager.clone();
        let options = SetOptions::default().with_expiration(redis::SetExpiry::EX(exp_sec));
        let key = format!("order:{}", order.order_id);
        let value = serde_json::to_string(order)?;
        let result: Option<String> = conn.set_options(key, value, options).await?;

        Ok(result.as_deref() == Some("OK"))
    }

    pub async fn push(&self, order: &Order, exp_sec: u64) -> Result<bool, CacheError> {
        self.add_order(order, exp_sec).await
    }

    pub async fn push_new_order(&self, new_order: &NewOrder, exp_sec: u64) -> Result<bool, CacheError> {
        let order = Order::from(new_order.clone());
        self.add_order(&order, exp_sec).await
    }

    pub async fn get_order(&self, order_id: OrderId) -> Result<Option<Order>, CacheError> {
        let mut conn = self.connection_manager.clone();
        let key = format!("order:{order_id}");
        let result: Option<String> = conn.get(key).await?;

        match result {
            Some(json_str) => {
                let order: Order = serde_json::from_str(&json_str)?;
                Ok(Some(order))
            },
            None => Ok(None),
        }
    }

    pub async fn delete_order(&self, order_id: OrderId) -> Result<bool, CacheError> {
        let mut conn = self.connection_manager.clone();
        let key = format!("order:{order_id}");
        let result: u32 = conn.del(key).await?;

        Ok(result > 0)
    }

    pub async fn order_cancelled(&self, order_id: OrderId) -> Result<bool, CacheError> {
        self.delete_order(order_id).await
    }

    pub async fn expire_order(&self, order_id: OrderId, exp_sec: u64) -> Result<bool, CacheError> {
        let mut conn = self.connection_manager.clone();
        let key = format!("order:{order_id}");
        let result: bool = conn.expire(key, exp_sec as i64).await?;

        Ok(result)
    }

    pub async fn modify_order(&self, modify: &ModifyOrder) -> Result<Option<Order>, CacheError> {
        let mut conn = self.connection_manager.clone();
        let key = format!("order:{}", modify.order_id);

        let Some(mut order) = self.get_order(modify.order_id).await? else {
            return Ok(None);
        };

        order.apply_modification(modify);

        let value = serde_json::to_string(&order)?;
        let options = SetOptions::default().with_expiration(redis::SetExpiry::KEEPTTL);
        let _: Option<String> = conn.set_options(key, value, options).await?;

        Ok(Some(order))
    }

    pub async fn order_modified(&self, modify: &ModifyOrder) -> Result<Option<Order>, CacheError> {
        self.modify_order(modify).await
    }

    pub async fn modify_order_with_ttl(&self, modify: &ModifyOrder, exp_sec: u64) -> Result<Option<Order>, CacheError> {
        let Some(mut order) = self.get_order(modify.order_id).await? else {
            return Ok(None);
        };

        order.apply_modification(modify);
        self.add_order(&order, exp_sec).await?;

        Ok(Some(order))
    }

    pub async fn update_order(&self, order: &Order, exp_sec: Option<u64>) -> Result<bool, CacheError> {
        match exp_sec {
            Some(ttl) => self.add_order(order, ttl).await,
            None => {
                let mut conn = self.connection_manager.clone();
                let key = format!("order:{}", order.order_id);
                let value = serde_json::to_string(order)?;
                let options = SetOptions::default().with_expiration(redis::SetExpiry::KEEPTTL);
                let result: Option<String> = conn.set_options(key, value, options).await?;
                Ok(result.as_deref() == Some("OK"))
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use domain::{OrderType, Side};

    use super::*;

    #[test]
    fn test_order_serialization_roundtrip() {
        let order = Order::new(101, 202, 1, Side::Buy, 50000, 10, OrderType::GoodTillCancel);
        let serialized = serde_json::to_string(&order).expect("Serialization failed");
        let deserialized: Order = serde_json::from_str(&serialized).expect("Deserialization failed");

        assert_eq!(order.order_id, deserialized.order_id);
        assert_eq!(order.user_id, deserialized.user_id);
        assert_eq!(order.asset_id, deserialized.asset_id);
        assert_eq!(order.price, deserialized.price);
        assert_eq!(order.initial_quantity, deserialized.initial_quantity);
        assert_eq!(order.remaining_quantity, deserialized.remaining_quantity);
        assert_eq!(order.order_type, deserialized.order_type);
        assert_eq!(order.side, deserialized.side);
    }

    #[test]
    fn test_apply_modification() {
        let mut order = Order::new(101, 202, 1, Side::Buy, 50000, 10, OrderType::GoodTillCancel);
        let modify = ModifyOrder {
            order_type: OrderType::FillAndKill,
            order_id: 101,
            side: Side::Sell,
            price: 52000,
            quantity: 5,
        };

        order.apply_modification(&modify);
        assert_eq!(order.order_type, OrderType::FillAndKill);
        assert_eq!(order.side, Side::Sell);
        assert_eq!(order.price, Some(52000));
        assert_eq!(order.initial_quantity, 5);
        assert_eq!(order.remaining_quantity, 5);
    }

    #[test]
    fn test_cache_error_from_serde() {
        let err = serde_json::from_str::<Order>("invalid json").unwrap_err();
        let cache_err: CacheError = err.into();
        assert!(matches!(cache_err, CacheError::Serialization(_)));
    }
}
