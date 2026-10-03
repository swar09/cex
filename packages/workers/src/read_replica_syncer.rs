use std::{
    thread,
    time::{Duration, Instant},
};

use cache::Cache;
use domain::{Order, OrderType, Side};
use engine::events::{EventConsumer, OrderBookEvent};
use fxhash::FxHashMap;

pub fn orderbook_read_replica_syncer(
    mut consumer: EventConsumer,
    redis_client: redis::Client,
    map: FxHashMap<usize, usize>,
) -> Result<thread::JoinHandle<()>, redis::RedisError> {
    let _ = redis_client.get_connection()?;
    let handle = thread::Builder::new()
        .name("orderbook-read-replica-syncer".to_string())
        .spawn(move || {
            let mut conn = match redis_client.get_connection() {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("[orderbook-read-replica-syncer] redis connection error: {e}");
                    return;
                },
            };
            let int_to_ext: FxHashMap<usize, usize> = map.into_iter().map(|(ext, int)| (int, ext)).collect();
            let mut pipe = redis::pipe();
            let mut pending_ops = 0usize;
            let mut last_flush = Instant::now();
            let flush_interval = Duration::from_millis(5);

            loop {
                let poll_result = consumer.poll();
                match poll_result {
                    Ok(events) => {
                        for event in events {
                            match event {
                                OrderBookEvent::OrderPlaced(_, symbol, ev) => {
                                    let order = Order {
                                        order_id: ev.order_id,
                                        user_id: ev.user_id,
                                        asset_id: symbol.get_quantity_unit().asset_id(),
                                        price: Some(ev.price),
                                        initial_quantity: ev.quantity,
                                        remaining_quantity: ev.quantity,
                                        order_type: ev.order_type,
                                        side: ev.side,
                                    };
                                    let order_key = Cache::replica_order_key(ev.order_id);
                                    if let Ok(val) = serde_json::to_string(&order) {
                                        pipe.set(order_key, val);
                                        pending_ops += 1;
                                    }
                                    let book_key = Cache::book_price_level_key(symbol.as_str(), Cache::side_to_str(ev.side), ev.price);
                                    pipe.rpush(book_key, ev.order_id);
                                    pending_ops += 1;
                                },
                                OrderBookEvent::TradeExecuted(_, symbol, ev) => {
                                    let cost = ev.price.saturating_mul(ev.quantity as u64);
                                    let maker_key = Cache::replica_order_key(ev.maker_order_id);
                                    if ev.maker_remaining_qty == 0 {
                                        pipe.del(&maker_key);
                                        let book_key = Cache::book_price_level_key(symbol.as_str(), Cache::side_to_str(ev.maker_side), ev.price);
                                        pipe.lrem(book_key, 1, ev.maker_order_id);
                                        pending_ops += 2;
                                    } else {
                                        let maker_order = Order {
                                            order_id: ev.maker_order_id,
                                            user_id: ev.maker_user_id,
                                            asset_id: symbol.get_quantity_unit().asset_id(),
                                            price: Some(ev.price),
                                            initial_quantity: ev.quantity + ev.maker_remaining_qty,
                                            remaining_quantity: ev.maker_remaining_qty,
                                            order_type: OrderType::GoodTillCancel,
                                            side: ev.maker_side,
                                        };
                                        if let Ok(val) = serde_json::to_string(&maker_order) {
                                            pipe.set(maker_key, val);
                                            pending_ops += 1;
                                        }
                                    }
                                    if ev.taker_remaining_qty == 0 {
                                        let taker_key = Cache::replica_order_key(ev.taker_order_id);
                                        pipe.del(taker_key);
                                        pending_ops += 1;
                                    }
                                    let (buyer_id, seller_id) = match ev.taker_side {
                                        Side::Buy => (ev.taker_user_id, ev.maker_user_id),
                                        Side::Sell => (ev.maker_user_id, ev.taker_user_id),
                                    };
                                    let buyer_ext = int_to_ext.get(&(buyer_id as usize)).copied().unwrap_or(buyer_id as usize);
                                    let seller_ext = int_to_ext.get(&(seller_id as usize)).copied().unwrap_or(seller_id as usize);

                                    let buyer_bal_key = Cache::account_balance_key(buyer_ext);
                                    pipe.decr(buyer_bal_key, cost);
                                    let buyer_hold_key = Cache::account_holding_key(buyer_ext, symbol.as_str());
                                    pipe.incr(buyer_hold_key, ev.quantity as u64);

                                    let seller_bal_key = Cache::account_balance_key(seller_ext);
                                    pipe.incr(seller_bal_key, cost);
                                    let seller_hold_key = Cache::account_holding_key(seller_ext, symbol.as_str());
                                    pipe.decr(seller_hold_key, ev.quantity as u64);
                                    pending_ops += 4;
                                },
                                OrderBookEvent::OrderCancelled(_, symbol, ev) => {
                                    let order_key = Cache::replica_order_key(ev.order_id);
                                    pipe.del(order_key);
                                    let book_key = Cache::book_price_level_key(symbol.as_str(), Cache::side_to_str(ev.side), ev.price);
                                    pipe.lrem(book_key, 1, ev.order_id);
                                    pending_ops += 2;
                                },
                                OrderBookEvent::OrderModified(_, symbol, ev) => {
                                    let old_book_key = Cache::book_price_level_key(symbol.as_str(), Cache::side_to_str(ev.side), ev.old_price);
                                    pipe.lrem(old_book_key, 1, ev.order_id);
                                    let new_book_key = Cache::book_price_level_key(symbol.as_str(), Cache::side_to_str(ev.side), ev.new_price);
                                    pipe.rpush(new_book_key, ev.order_id);
                                    let order = Order {
                                        order_id: ev.order_id,
                                        user_id: ev.user_id,
                                        asset_id: symbol.get_quantity_unit().asset_id(),
                                        price: Some(ev.new_price),
                                        initial_quantity: ev.new_qty,
                                        remaining_quantity: ev.new_qty,
                                        order_type: OrderType::GoodTillCancel,
                                        side: ev.side,
                                    };
                                    let order_key = Cache::replica_order_key(ev.order_id);
                                    if let Ok(val) = serde_json::to_string(&order) {
                                        pipe.set(order_key, val);
                                        pending_ops += 1;
                                    }
                                    pending_ops += 2;
                                },
                                OrderBookEvent::OrdersExpired(_, symbol, expired) => {
                                    for (order_id, _, price, side, _) in expired {
                                        let order_key = Cache::replica_order_key(order_id);
                                        pipe.del(order_key);
                                        let book_key = Cache::book_price_level_key(symbol.as_str(), Cache::side_to_str(side), price);
                                        pipe.lrem(book_key, 1, order_id);
                                        pending_ops += 2;
                                    }
                                },
                                _ => {},
                            }
                        }
                    },
                    Err(_) => {
                        thread::sleep(Duration::from_millis(1));
                    },
                }

                if pending_ops > 0 && (last_flush.elapsed() >= flush_interval || pending_ops >= 100) {
                    if let Err(e) = pipe.query::<()>(&mut conn) {
                        eprintln!("[orderbook-read-replica-syncer] redis pipeline error: {e}");
                    }
                    pipe = redis::pipe();
                    pending_ops = 0;
                    last_flush = Instant::now();
                }
            }
        })
        .map_err(redis::RedisError::from)?;

    Ok(handle)
}

pub fn orderbook_read_replica_syncer_from_url(
    consumer: EventConsumer,
    redis_url: &str,
    map: FxHashMap<usize, usize>,
) -> Result<thread::JoinHandle<()>, redis::RedisError> {
    let client = redis::Client::open(redis_url)?;
    orderbook_read_replica_syncer(consumer, client, map)
}

#[cfg(test)]
mod tests {
    use disruptor::BusySpin;
    use domain::{OrderPlacedEvent, OrderType, Side, Symbol, TradeExecutedEvent};
    use engine::events::EventEnvelope;

    use super::*;

    #[test]
    fn test_id_map_inversion() {
        let mut map = FxHashMap::default();
        map.insert(101, 1);
        map.insert(102, 2);
        let int_to_ext: FxHashMap<usize, usize> = map.into_iter().map(|(ext, int)| (int, ext)).collect();
        assert_eq!(int_to_ext.get(&1), Some(&101));
        assert_eq!(int_to_ext.get(&2), Some(&102));
        assert_eq!(int_to_ext.get(&3), None);
    }

    #[test]
    fn test_order_placed_projection_keys() {
        let ev = OrderPlacedEvent {
            order_id: 55,
            user_id: 10,
            side: Side::Buy,
            price: 50000,
            quantity: 2,
            order_type: OrderType::GoodTillCancel,
        };
        let order_key = Cache::replica_order_key(ev.order_id);
        let book_key = Cache::book_price_level_key(Symbol::BtcUsdt.as_str(), Cache::side_to_str(ev.side), ev.price);
        assert_eq!(order_key, "orders:55");
        assert_eq!(book_key, "book:BTC-USDT:bids:50000");
    }

    #[test]
    fn test_trade_executed_projection_balance_keys() {
        let ev = TradeExecutedEvent {
            trade_id: 1,
            maker_order_id: 10,
            taker_order_id: 20,
            maker_user_id: 1,
            taker_user_id: 2,
            maker_side: Side::Sell,
            taker_side: Side::Buy,
            price: 50000,
            quantity: 1,
            maker_remaining_qty: 0,
            taker_remaining_qty: 0,
        };
        let mut map = FxHashMap::default();
        map.insert(1001, 1);
        map.insert(2002, 2);
        let int_to_ext: FxHashMap<usize, usize> = map.into_iter().map(|(ext, int)| (int, ext)).collect();

        let (buyer_id, seller_id) = match ev.taker_side {
            Side::Buy => (ev.taker_user_id, ev.maker_user_id),
            Side::Sell => (ev.maker_user_id, ev.taker_user_id),
        };
        let buyer_ext = int_to_ext.get(&(buyer_id as usize)).copied().unwrap_or(buyer_id as usize);
        let seller_ext = int_to_ext.get(&(seller_id as usize)).copied().unwrap_or(seller_id as usize);

        assert_eq!(buyer_ext, 2002);
        assert_eq!(seller_ext, 1001);
        assert_eq!(Cache::account_balance_key(buyer_ext), "account:2002");
        assert_eq!(Cache::account_balance_key(seller_ext), "account:1001");
        assert_eq!(
            Cache::account_holding_key(buyer_ext, Symbol::BtcUsdt.as_str()),
            "account:2002:holdings:BTC-USDT"
        );
        assert_eq!(
            Cache::account_holding_key(seller_ext, Symbol::BtcUsdt.as_str()),
            "account:1001:holdings:BTC-USDT"
        );
    }

    #[test]
    fn test_syncer_fails_on_unreachable_redis() {
        let client = redis::Client::open("redis://127.0.0.1:65534").unwrap();
        let (poller, builder) = disruptor::build_single_producer(8, EventEnvelope::empty, BusySpin)
            .with_multi_consumer()
            .new_event_poller();
        let _producer = builder.build();
        let consumer = EventConsumer::new(poller);
        let res = orderbook_read_replica_syncer(consumer, client, FxHashMap::default());
        assert!(res.is_err());
    }
}
