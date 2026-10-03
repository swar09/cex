pub mod error;

use std::time::Duration;

pub use domain::{ModifyOrder, NewOrder, Order, OrderId, Price, Quantity, Side, Symbol};
pub use error::CacheError;
use jsonwebtoken::jwk::Jwk;

pub type ExternalUserId = usize;
pub type Balance = u64;
use redis::{
    AsyncCommands, Client, SetOptions,
    aio::{ConnectionManager, ConnectionManagerConfig},
};
use uuid::Uuid;

#[derive(Clone)]
pub struct Cache {
    pub connection_manager: ConnectionManager,
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

    pub fn replica_order_key(order_id: OrderId) -> String {
        format!("orders:{order_id}")
    }

    pub fn book_price_level_key(symbol: &str, side: &str, price_level: Price) -> String {
        format!("book:{symbol}:{side}:{price_level}")
    }

    pub fn account_balance_key(external_user_id: ExternalUserId) -> String {
        format!("account:{external_user_id}")
    }

    pub fn account_holding_key(external_user_id: ExternalUserId, symbol: &str) -> String {
        format!("account:{external_user_id}:holdings:{symbol}")
    }

    pub fn side_to_str(side: Side) -> &'static str {
        match side {
            Side::Buy => "bids",
            Side::Sell => "asks",
        }
    }

    pub async fn set_replica_order(&self, order: &Order) -> Result<bool, CacheError> {
        let mut conn = self.connection_manager.clone();
        let key = Self::replica_order_key(order.order_id);
        let value = serde_json::to_string(order)?;
        let result: Option<String> = conn.set(key, value).await?;
        Ok(result.as_deref() == Some("OK") || result.is_none())
    }

    pub async fn get_replica_order(&self, order_id: OrderId) -> Result<Option<Order>, CacheError> {
        let mut conn = self.connection_manager.clone();
        let key = Self::replica_order_key(order_id);
        let result: Option<String> = conn.get(key).await?;
        match result {
            Some(json_str) => {
                let order: Order = serde_json::from_str(&json_str)?;
                Ok(Some(order))
            },
            None => Ok(None),
        }
    }

    pub async fn update_replica_order(&self, order: &Order) -> Result<bool, CacheError> {
        self.set_replica_order(order).await
    }

    pub async fn delete_replica_order(&self, order_id: OrderId) -> Result<bool, CacheError> {
        let mut conn = self.connection_manager.clone();
        let key = Self::replica_order_key(order_id);
        let result: u32 = conn.del(key).await?;
        Ok(result > 0)
    }

    pub async fn modify_replica_order(&self, modify: &ModifyOrder) -> Result<Option<Order>, CacheError> {
        let Some(mut order) = self.get_replica_order(modify.order_id).await? else {
            return Ok(None);
        };
        order.apply_modification(modify);
        self.set_replica_order(&order).await?;
        Ok(Some(order))
    }

    pub async fn batch_set_replica_orders(&self, orders: &[Order]) -> Result<(), CacheError> {
        if orders.is_empty() {
            return Ok(());
        }
        let mut conn = self.connection_manager.clone();
        let mut pipe = redis::pipe();
        for order in orders {
            let key = Self::replica_order_key(order.order_id);
            let value = serde_json::to_string(order)?;
            pipe.set(key, value);
        }
        let _: () = pipe.query_async(&mut conn).await?;
        Ok(())
    }

    pub async fn batch_delete_replica_orders(&self, order_ids: &[OrderId]) -> Result<(), CacheError> {
        if order_ids.is_empty() {
            return Ok(());
        }
        let mut conn = self.connection_manager.clone();
        let mut pipe = redis::pipe();
        for order_id in order_ids {
            let key = Self::replica_order_key(*order_id);
            pipe.del(key);
        }
        let _: () = pipe.query_async(&mut conn).await?;
        Ok(())
    }

    pub async fn set_book_order_ids(&self, symbol: &str, side: &str, price_level: Price, order_ids: &[OrderId]) -> Result<(), CacheError> {
        let mut conn = self.connection_manager.clone();
        let key = Self::book_price_level_key(symbol, side, price_level);
        let mut pipe = redis::pipe();
        pipe.del(&key);
        if !order_ids.is_empty() {
            pipe.rpush(&key, order_ids);
        }
        let _: () = pipe.query_async(&mut conn).await?;
        Ok(())
    }

    pub async fn push_book_order_id(&self, symbol: &str, side: &str, price_level: Price, order_id: OrderId) -> Result<(), CacheError> {
        let mut conn = self.connection_manager.clone();
        let key = Self::book_price_level_key(symbol, side, price_level);
        let _: () = conn.rpush(key, order_id).await?;
        Ok(())
    }

    pub async fn push_book_order_ids(&self, symbol: &str, side: &str, price_level: Price, order_ids: &[OrderId]) -> Result<(), CacheError> {
        if order_ids.is_empty() {
            return Ok(());
        }
        let mut conn = self.connection_manager.clone();
        let key = Self::book_price_level_key(symbol, side, price_level);
        let _: () = conn.rpush(key, order_ids).await?;
        Ok(())
    }

    pub async fn get_book_order_ids(&self, symbol: &str, side: &str, price_level: Price) -> Result<Vec<OrderId>, CacheError> {
        let mut conn = self.connection_manager.clone();
        let key = Self::book_price_level_key(symbol, side, price_level);
        let ids: Vec<OrderId> = conn.lrange(key, 0, -1).await?;
        Ok(ids)
    }

    pub async fn remove_book_order_id(&self, symbol: &str, side: &str, price_level: Price, order_id: OrderId) -> Result<bool, CacheError> {
        let mut conn = self.connection_manager.clone();
        let key = Self::book_price_level_key(symbol, side, price_level);
        let count: u32 = conn.lrem(key, 0, order_id).await?;
        Ok(count > 0)
    }

    pub async fn pop_book_order_id(&self, symbol: &str, side: &str, price_level: Price) -> Result<Option<OrderId>, CacheError> {
        let mut conn = self.connection_manager.clone();
        let key = Self::book_price_level_key(symbol, side, price_level);
        let id: Option<OrderId> = conn.lpop(key, None).await?;
        Ok(id)
    }

    pub async fn delete_book_price_level(&self, symbol: &str, side: &str, price_level: Price) -> Result<bool, CacheError> {
        let mut conn = self.connection_manager.clone();
        let key = Self::book_price_level_key(symbol, side, price_level);
        let count: u32 = conn.del(key).await?;
        Ok(count > 0)
    }

    pub async fn set_book_order_ids_typed(&self, symbol: Symbol, side: Side, price_level: Price, order_ids: &[OrderId]) -> Result<(), CacheError> {
        self.set_book_order_ids(symbol.as_str(), Self::side_to_str(side), price_level, order_ids)
            .await
    }

    pub async fn push_book_order_id_typed(&self, symbol: Symbol, side: Side, price_level: Price, order_id: OrderId) -> Result<(), CacheError> {
        self.push_book_order_id(symbol.as_str(), Self::side_to_str(side), price_level, order_id)
            .await
    }

    pub async fn get_book_order_ids_typed(&self, symbol: Symbol, side: Side, price_level: Price) -> Result<Vec<OrderId>, CacheError> {
        self.get_book_order_ids(symbol.as_str(), Self::side_to_str(side), price_level).await
    }

    pub async fn remove_book_order_id_typed(&self, symbol: Symbol, side: Side, price_level: Price, order_id: OrderId) -> Result<bool, CacheError> {
        self.remove_book_order_id(symbol.as_str(), Self::side_to_str(side), price_level, order_id)
            .await
    }

    pub async fn pop_book_order_id_typed(&self, symbol: Symbol, side: Side, price_level: Price) -> Result<Option<OrderId>, CacheError> {
        self.pop_book_order_id(symbol.as_str(), Self::side_to_str(side), price_level).await
    }

    pub async fn delete_book_price_level_typed(&self, symbol: Symbol, side: Side, price_level: Price) -> Result<bool, CacheError> {
        self.delete_book_price_level(symbol.as_str(), Self::side_to_str(side), price_level).await
    }

    pub async fn set_account_balance(&self, external_user_id: ExternalUserId, balance: Balance) -> Result<bool, CacheError> {
        let mut conn = self.connection_manager.clone();
        let key = Self::account_balance_key(external_user_id);
        let result: Option<String> = conn.set(key, balance).await?;
        Ok(result.as_deref() == Some("OK") || result.is_none())
    }

    pub async fn get_account_balance(&self, external_user_id: ExternalUserId) -> Result<Option<Balance>, CacheError> {
        let mut conn = self.connection_manager.clone();
        let key = Self::account_balance_key(external_user_id);
        let result: Option<Balance> = conn.get(key).await?;
        Ok(result)
    }

    pub async fn update_account_balance(&self, external_user_id: ExternalUserId, balance: Balance) -> Result<bool, CacheError> {
        self.set_account_balance(external_user_id, balance).await
    }

    pub async fn delete_account_balance(&self, external_user_id: ExternalUserId) -> Result<bool, CacheError> {
        let mut conn = self.connection_manager.clone();
        let key = Self::account_balance_key(external_user_id);
        let count: u32 = conn.del(key).await?;
        Ok(count > 0)
    }

    pub async fn increment_account_balance(&self, external_user_id: ExternalUserId, amount: Balance) -> Result<Balance, CacheError> {
        let mut conn = self.connection_manager.clone();
        let key = Self::account_balance_key(external_user_id);
        let new_balance: Balance = conn.incr(key, amount).await?;
        Ok(new_balance)
    }

    pub async fn decrement_account_balance(&self, external_user_id: ExternalUserId, amount: Balance) -> Result<Balance, CacheError> {
        let mut conn = self.connection_manager.clone();
        let key = Self::account_balance_key(external_user_id);
        let new_balance: Balance = conn.decr(key, amount).await?;
        Ok(new_balance)
    }

    pub async fn set_account_holding(&self, external_user_id: ExternalUserId, symbol: &str, quantity: Quantity) -> Result<bool, CacheError> {
        let mut conn = self.connection_manager.clone();
        let key = Self::account_holding_key(external_user_id, symbol);
        let result: Option<String> = conn.set(key, quantity as u64).await?;
        Ok(result.as_deref() == Some("OK") || result.is_none())
    }

    pub async fn get_account_holding(&self, external_user_id: ExternalUserId, symbol: &str) -> Result<Option<Quantity>, CacheError> {
        let mut conn = self.connection_manager.clone();
        let key = Self::account_holding_key(external_user_id, symbol);
        let result: Option<u64> = conn.get(key).await?;
        Ok(result.map(|q| q as Quantity))
    }

    pub async fn update_account_holding(&self, external_user_id: ExternalUserId, symbol: &str, quantity: Quantity) -> Result<bool, CacheError> {
        self.set_account_holding(external_user_id, symbol, quantity).await
    }

    pub async fn delete_account_holding(&self, external_user_id: ExternalUserId, symbol: &str) -> Result<bool, CacheError> {
        let mut conn = self.connection_manager.clone();
        let key = Self::account_holding_key(external_user_id, symbol);
        let count: u32 = conn.del(key).await?;
        Ok(count > 0)
    }

    pub async fn increment_account_holding(
        &self,
        external_user_id: ExternalUserId,
        symbol: &str,
        quantity: Quantity,
    ) -> Result<Quantity, CacheError> {
        let mut conn = self.connection_manager.clone();
        let key = Self::account_holding_key(external_user_id, symbol);
        let new_qty: u64 = conn.incr(key, quantity as u64).await?;
        Ok(new_qty as Quantity)
    }

    pub async fn decrement_account_holding(
        &self,
        external_user_id: ExternalUserId,
        symbol: &str,
        quantity: Quantity,
    ) -> Result<Quantity, CacheError> {
        let mut conn = self.connection_manager.clone();
        let key = Self::account_holding_key(external_user_id, symbol);
        let new_qty: u64 = conn.decr(key, quantity as u64).await?;
        Ok(new_qty as Quantity)
    }

    pub async fn set_account_holding_typed(&self, external_user_id: ExternalUserId, symbol: Symbol, quantity: Quantity) -> Result<bool, CacheError> {
        self.set_account_holding(external_user_id, symbol.as_str(), quantity).await
    }

    pub async fn get_account_holding_typed(&self, external_user_id: ExternalUserId, symbol: Symbol) -> Result<Option<Quantity>, CacheError> {
        self.get_account_holding(external_user_id, symbol.as_str()).await
    }

    pub async fn update_account_holding_typed(
        &self,
        external_user_id: ExternalUserId,
        symbol: Symbol,
        quantity: Quantity,
    ) -> Result<bool, CacheError> {
        self.update_account_holding(external_user_id, symbol.as_str(), quantity).await
    }

    pub async fn delete_account_holding_typed(&self, external_user_id: ExternalUserId, symbol: Symbol) -> Result<bool, CacheError> {
        self.delete_account_holding(external_user_id, symbol.as_str()).await
    }

    pub async fn increment_account_holding_typed(
        &self,
        external_user_id: ExternalUserId,
        symbol: Symbol,
        quantity: Quantity,
    ) -> Result<Quantity, CacheError> {
        self.increment_account_holding(external_user_id, symbol.as_str(), quantity).await
    }

    pub async fn decrement_account_holding_typed(
        &self,
        external_user_id: ExternalUserId,
        symbol: Symbol,
        quantity: Quantity,
    ) -> Result<Quantity, CacheError> {
        self.decrement_account_holding(external_user_id, symbol.as_str(), quantity).await
    }

    pub async fn execute_pipeline<T: redis::FromRedisValue>(&self, pipe: &redis::Pipeline) -> Result<T, CacheError> {
        let mut conn = self.connection_manager.clone();
        let res = pipe.query_async(&mut conn).await?;
        Ok(res)
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

    #[test]
    fn test_replica_keys_and_side() {
        assert_eq!(Cache::replica_order_key(1001), "orders:1001");
        assert_eq!(Cache::book_price_level_key("BTC-USDT", "asks", 50000), "book:BTC-USDT:asks:50000");
        assert_eq!(Cache::book_price_level_key("BTC-USDT", "bids", 49000), "book:BTC-USDT:bids:49000");
        assert_eq!(Cache::account_balance_key(42), "account:42");
        assert_eq!(Cache::account_holding_key(42, "BTC-USDT"), "account:42:holdings:BTC-USDT");
        assert_eq!(Cache::side_to_str(Side::Buy), "bids");
        assert_eq!(Cache::side_to_str(Side::Sell), "asks");
    }

    #[test]
    fn test_replica_order_keys() {
        assert_eq!(Cache::replica_order_key(0), "orders:0");
        assert_eq!(Cache::replica_order_key(1), "orders:1");
        assert_eq!(Cache::replica_order_key(999999), "orders:999999");
        assert_eq!(Cache::replica_order_key(u64::MAX), format!("orders:{}", u64::MAX));
    }

    #[test]
    fn test_book_price_level_keys() {
        assert_eq!(Cache::book_price_level_key("BTC-USDT", "asks", 50000), "book:BTC-USDT:asks:50000");
        assert_eq!(Cache::book_price_level_key("ETH-INR", "bids", 250000), "book:ETH-INR:bids:250000");
        assert_eq!(
            Cache::book_price_level_key(Symbol::SolUsdt.as_str(), Cache::side_to_str(Side::Buy), 150),
            "book:SOL-USDT:bids:150"
        );
        assert_eq!(
            Cache::book_price_level_key(Symbol::SolUsdt.as_str(), Cache::side_to_str(Side::Sell), 155),
            "book:SOL-USDT:asks:155"
        );
        assert_eq!(
            Cache::book_price_level_key(Symbol::BtcUsdc.as_str(), Cache::side_to_str(Side::Buy), 60000),
            "book:BTC-USDC:bids:60000"
        );
    }

    #[test]
    fn test_account_balance_keys() {
        assert_eq!(Cache::account_balance_key(0), "account:0");
        assert_eq!(Cache::account_balance_key(1), "account:1");
        assert_eq!(Cache::account_balance_key(42), "account:42");
        assert_eq!(Cache::account_balance_key(100500), "account:100500");
    }

    #[test]
    fn test_account_holding_keys() {
        assert_eq!(Cache::account_holding_key(1, "BTC-USDT"), "account:1:holdings:BTC-USDT");
        assert_eq!(Cache::account_holding_key(50, Symbol::EthInr.as_str()), "account:50:holdings:ETH-INR");
        assert_eq!(Cache::account_holding_key(0, "USDT"), "account:0:holdings:USDT");
        assert_eq!(Cache::account_holding_key(99, Symbol::BnbUsdt.as_str()), "account:99:holdings:BNB-USDT");
    }

    #[test]
    fn test_side_to_str_mapping() {
        assert_eq!(Cache::side_to_str(Side::Buy), "bids");
        assert_eq!(Cache::side_to_str(Side::Sell), "asks");
    }

    #[test]
    fn test_order_types_serialization() {
        let types = [
            OrderType::GoodTillCancel,
            OrderType::FillAndKill,
            OrderType::FillOrKill,
            OrderType::GoodForDay,
            OrderType::Market,
        ];
        for ot in types {
            let order = Order::new(1, 2, 3, Side::Buy, 100, 10, ot);
            let json = serde_json::to_string(&order).unwrap();
            let deserialized: Order = serde_json::from_str(&json).unwrap();
            assert_eq!(order, deserialized);
        }
    }

    #[test]
    fn test_batch_orders_serialization() {
        let orders = vec![
            Order::new(101, 1, 1, Side::Buy, 50000, 2, OrderType::GoodTillCancel),
            Order::new(102, 2, 1, Side::Sell, 50100, 3, OrderType::GoodTillCancel),
            Order::new(103, 3, 2, Side::Buy, 3000, 5, OrderType::FillAndKill),
        ];
        for order in &orders {
            let key = Cache::replica_order_key(order.order_id);
            let serialized = serde_json::to_string(order).unwrap();
            let deserialized: Order = serde_json::from_str(&serialized).unwrap();
            assert_eq!(*order, deserialized);
            assert_eq!(key, format!("orders:{}", order.order_id));
        }
        let order_ids: Vec<OrderId> = orders.iter().map(|o| o.order_id).collect();
        let keys: Vec<String> = order_ids.iter().map(|id| Cache::replica_order_key(*id)).collect();
        assert_eq!(keys, vec!["orders:101", "orders:102", "orders:103"]);
    }

    #[test]
    fn test_balance_and_quantity_numeric_conversion() {
        let balance: Balance = 9876543210;
        let serialized = balance.to_string();
        let parsed: Balance = serialized.parse().unwrap();
        assert_eq!(balance, parsed);

        let quantity: Quantity = 54321;
        let stored_as_u64 = quantity as u64;
        let restored = stored_as_u64 as Quantity;
        assert_eq!(quantity, restored);
    }

    #[test]
    fn test_cache_error_display_and_variants() {
        let not_found = CacheError::NotFound("test_key".to_string());
        assert_eq!(format!("{not_found}"), "Key not found: test_key");

        let fetch_err = CacheError::FetchError("network down".to_string());
        assert_eq!(format!("{fetch_err}"), "Fetch error: network down");

        let parse_err = "abc".parse::<u64>().unwrap_err();
        let cache_parse_err = CacheError::from(parse_err);
        assert!(matches!(cache_parse_err, CacheError::ParseInt(_)));
    }

    #[test]
    fn test_pipeline_construction_without_server() {
        let mut pipe = redis::pipe();
        pipe.set(Cache::replica_order_key(101), "value");
        pipe.del(Cache::replica_order_key(102));
        pipe.rpush(Cache::book_price_level_key("BTC-USDT", "asks", 50000), 101);
        pipe.lrem(Cache::book_price_level_key("BTC-USDT", "asks", 50000), 0, 101);
        pipe.set(Cache::account_balance_key(42), 1000000);
        pipe.set(Cache::account_holding_key(42, "BTC-USDT"), 500);
    }

    #[test]
    fn test_invalid_redis_url_fails() {
        let result = Client::open("invalid_url_without_scheme");
        assert!(result.is_err());
    }
}
