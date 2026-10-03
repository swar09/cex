use disruptor::{EventPoller, MultiConsumerBarrier, Polling, Producer, SingleProducer, SingleProducerBarrier};
use domain::{
    ExpredOrders, ModifyOrderRejectedEvent, OrderCancelledEvent, OrderId, OrderModifiedEvent, OrderPlacedEvent, OrderRejectedEvent, OrderType, Price,
    Quantity, Side, Symbol, TradeExecutedEvent,
};
use serde::{Deserialize, Serialize};

use crate::exchange::Sequence;
#[derive(Debug, Clone, Serialize)]
pub struct OrderbookEventLog {
    pub event_type: Option<String>,
    pub sequence_no: Option<u64>,
    pub symbol: Option<Symbol>,
    pub order_id: Option<OrderId>,
    pub price: Option<Price>,
    pub quantity: Option<Quantity>,
    pub order_type: Option<OrderType>,
    pub order_side: Option<Side>,
    pub order_ids: Option<Vec<u64>>,
    pub error_code: Option<u64>,
}

// orderbook events single producer multiple consumers
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub enum OrderBookEvent {
    // order placed
    OrderPlaced(Sequence, Symbol, OrderPlacedEvent),
    // trade occurred
    TradeExecuted(Sequence, Symbol, TradeExecutedEvent),
    // order cancelled
    OrderCancelled(Sequence, Symbol, OrderCancelledEvent),
    // order modified by trader
    OrderModified(Sequence, Symbol, OrderModifiedEvent),
    OrderRejected(Sequence, Symbol, OrderRejectedEvent), // order rejected by exchange
    // modify order request  rejected by exchange
    ModifyOrderRejected(Sequence, Symbol, ModifyOrderRejectedEvent),
    OrdersExpired(Sequence, Symbol, ExpredOrders), // orders expired
    MarketOpened(Sequence, Symbol),                // market opened
    MarketClosed(Sequence, Symbol),                // market closed
    Error(Sequence, u32),                          // error code in orderbook
}

impl OrderBookEvent {
    pub fn symbol(&self) -> Option<Symbol> {
        match self {
            Self::OrderPlaced(_, s, ..)
            | Self::TradeExecuted(_, s, ..)
            | Self::OrderCancelled(_, s, ..)
            | Self::OrderModified(_, s, ..)
            | Self::OrderRejected(_, s, ..)
            | Self::ModifyOrderRejected(_, s, ..)
            | Self::OrdersExpired(_, s, ..)
            | Self::MarketOpened(_, s)
            | Self::MarketClosed(_, s) => Some(*s),
            Self::Error(..) => None,
        }
    }

    pub fn sequence(&self) -> Sequence {
        match self {
            Self::OrderPlaced(seq, ..)
            | Self::TradeExecuted(seq, ..)
            | Self::OrderCancelled(seq, ..)
            | Self::OrderModified(seq, ..)
            | Self::OrderRejected(seq, ..)
            | Self::ModifyOrderRejected(seq, ..)
            | Self::OrdersExpired(seq, ..)
            | Self::MarketOpened(seq, ..)
            | Self::MarketClosed(seq, ..)
            | Self::Error(seq, ..) => *seq,
        }
    }

    pub fn to_log_data(&self) -> Option<OrderbookEventLog> {
        match self {
            Self::OrderPlaced(seq, symbol, ev) => {
                let event_type = self.as_str().to_string();

                Some(OrderbookEventLog {
                    event_type: Some(event_type),
                    sequence_no: Some(*seq),
                    symbol: Some(*symbol),
                    order_id: Some(ev.order_id),
                    price: Some(ev.price),
                    quantity: Some(ev.quantity),
                    order_type: Some(ev.order_type),
                    order_side: Some(ev.side),
                    order_ids: None,
                    error_code: None,
                })
            },

            Self::TradeExecuted(seq, symbol, ev) => {
                let event_type = self.as_str().to_string();

                Some(OrderbookEventLog {
                    event_type: Some(event_type),
                    sequence_no: Some(*seq),
                    symbol: Some(*symbol),
                    order_id: Some(ev.maker_order_id),
                    price: Some(ev.price),
                    quantity: Some(ev.quantity),
                    order_side: Some(ev.taker_side),
                    order_type: None,
                    order_ids: None,
                    error_code: None,
                })
            },

            Self::OrderCancelled(seq, symbol, ev) => {
                let event_type = self.as_str().to_string();
                Some(OrderbookEventLog {
                    event_type: Some(event_type),
                    sequence_no: Some(*seq),
                    symbol: Some(*symbol),
                    order_id: Some(ev.order_id),
                    order_type: None,
                    price: Some(ev.price),
                    order_side: Some(ev.side),
                    quantity: Some(ev.cancelled_qty),
                    order_ids: None,
                    error_code: None,
                })
            },

            Self::OrderModified(seq, symbol, ev) => {
                let event_type = self.as_str().to_string();

                Some(OrderbookEventLog {
                    event_type: Some(event_type),
                    sequence_no: Some(*seq),
                    symbol: Some(*symbol),
                    order_id: Some(ev.order_id),
                    price: Some(ev.new_price),
                    quantity: Some(ev.new_qty),
                    order_type: None,
                    order_side: Some(ev.side),
                    order_ids: None,
                    error_code: None,
                })
            },

            Self::OrderRejected(seq, symbol, ev) => {
                let event_type = self.as_str().to_string();
                Some(OrderbookEventLog {
                    event_type: Some(event_type),
                    sequence_no: Some(*seq),
                    symbol: Some(*symbol),
                    order_id: Some(ev.order_id),
                    order_type: None,
                    price: None,
                    order_side: None,
                    quantity: None,
                    order_ids: None,
                    error_code: None,
                })
            },

            Self::ModifyOrderRejected(seq, symbol, ev) => {
                let event_type = self.as_str().to_string();

                Some(OrderbookEventLog {
                    event_type: Some(event_type),
                    sequence_no: Some(*seq),
                    symbol: Some(*symbol),
                    order_id: Some(ev.order_id),
                    price: Some(ev.new_price),
                    quantity: Some(ev.new_qty),
                    order_type: None,
                    order_side: Some(ev.side),
                    order_ids: None,
                    error_code: None,
                })
            },

            Self::OrdersExpired(seq, symbol, expred_orders) => {
                let event_type = self.as_str().to_string();
                let order_ids: Vec<u64> = expred_orders.iter().map(|(id, ..)| *id).collect();
                Some(OrderbookEventLog {
                    event_type: Some(event_type),
                    sequence_no: Some(*seq),
                    symbol: Some(*symbol),
                    order_id: None,
                    price: None,
                    quantity: None,
                    order_type: None,
                    order_side: None,
                    order_ids: Some(order_ids),
                    error_code: None,
                })
            },

            Self::MarketOpened(_seq, _symbol) => {
                // TODO
                None
            },
            Self::MarketClosed(_seq, _symbol) => {
                // TODO
                None
            },
            Self::Error(_seq, _error_code) => {
                // TODO
                None
            },
        }
    }

    pub fn is_error(&self) -> bool {
        matches!(self, Self::Error(..))
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::MarketClosed(..) => "MarketClosed",
            Self::MarketOpened(..) => "MarketOpened",
            Self::OrderPlaced(..) => "OrderPlaced",
            Self::TradeExecuted(..) => "TradeExecuted",
            Self::OrderCancelled(..) => "OrderCancelled",
            Self::OrderModified(..) => "OrderModified",
            Self::OrderRejected(..) => "OrderRejected",
            Self::OrdersExpired(..) => "OrdersExpired",
            Self::ModifyOrderRejected(..) => "ModifyOrderRejected",
            Self::Error(..) => "ERROR",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct EventEnvelope {
    pub event: Option<OrderBookEvent>,
}

impl EventEnvelope {
    pub fn new(event: OrderBookEvent) -> Self {
        Self { event: Some(event) }
    }

    pub fn empty() -> Self {
        Self { event: None }
    }
}

pub struct EventDispatcher {
    pub producer: SingleProducer<EventEnvelope, MultiConsumerBarrier>, // multiple consumers configured
}

impl EventDispatcher {
    pub fn new(producer: SingleProducer<EventEnvelope, MultiConsumerBarrier>) -> Self {
        Self { producer }
    }

    // can stall producer if ring buffer is full
    pub fn publish(&mut self, event: OrderBookEvent) {
        self.producer.publish(|slot| slot.event = Some(event));
    }

    // returns error if ring buffer is full (non-blocking)
    pub fn try_send(&mut self, event: OrderBookEvent) -> Result<i64, disruptor::RingBufferFull> {
        self.producer.try_publish(|slot| slot.event = Some(event))
    }
}

pub struct EventConsumer {
    pub event_poller: EventPoller<EventEnvelope, SingleProducerBarrier>,
}

impl EventConsumer {
    pub fn new(event_poller: EventPoller<EventEnvelope, SingleProducerBarrier>) -> Self {
        Self { event_poller }
    }

    pub fn poll(&mut self) -> Result<Vec<OrderBookEvent>, Polling> {
        match self.event_poller.poll() {
            Ok(mut event_guard) => {
                let mut events = Vec::new();
                for slot in &mut event_guard {
                    if let Some(event) = &slot.event {
                        events.push(event.clone());
                    }
                }
                Ok(events)
            },
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use disruptor::{BusySpin, build_single_producer};
    use domain::{
        CancelReason, ModifyOrderRejectedEvent, OrderCancelledEvent, OrderModifiedEvent, OrderPlacedEvent, OrderRejectedEvent, OrderType,
        RejectReason, Side, TradeExecutedEvent,
    };

    use super::*;

    fn create_event_pipeline(buffer_size: usize) -> (EventDispatcher, EventConsumer) {
        let builder = build_single_producer(buffer_size, EventEnvelope::empty, BusySpin).with_multi_consumer();
        let (poller, builder) = builder.new_event_poller();
        let producer = builder.build();
        (EventDispatcher::new(producer), EventConsumer::new(poller))
    }

    #[test]
    fn test_orderbook_event_symbol_extraction() {
        let placed = OrderBookEvent::OrderPlaced(
            1,
            Symbol::BtcInr,
            OrderPlacedEvent {
                order_id: 1,
                user_id: 101,
                side: Side::Buy,
                price: 100,
                quantity: 10,
                order_type: OrderType::GoodTillCancel,
            },
        );
        assert_eq!(placed.symbol(), Some(Symbol::BtcInr));
        assert_eq!(placed.sequence(), 1);
        assert!(!placed.is_error());

        let trade = OrderBookEvent::TradeExecuted(
            2,
            Symbol::BtcInr,
            TradeExecutedEvent {
                trade_id: 1,
                maker_order_id: 1,
                taker_order_id: 2,
                maker_user_id: 101,
                taker_user_id: 102,
                maker_side: Side::Buy,
                taker_side: Side::Sell,
                price: 100,
                quantity: 5,
                maker_remaining_qty: 5,
                taker_remaining_qty: 0,
            },
        );
        assert_eq!(trade.symbol(), Some(Symbol::BtcInr));
        assert_eq!(trade.sequence(), 2);
        assert!(!trade.is_error());

        let cancelled = OrderBookEvent::OrderCancelled(
            3,
            Symbol::EthInr,
            OrderCancelledEvent {
                order_id: 2,
                user_id: 102,
                side: Side::Sell,
                price: 2_000,
                cancelled_qty: 5,
                reason: CancelReason::UserRequested,
            },
        );
        assert_eq!(cancelled.symbol(), Some(Symbol::EthInr));
        assert_eq!(cancelled.sequence(), 3);

        let modified = OrderBookEvent::OrderModified(
            4,
            Symbol::BtcUsdt,
            OrderModifiedEvent {
                order_id: 4,
                user_id: 104,
                side: Side::Buy,
                old_price: 50_000,
                new_price: 51_000,
                old_qty: 2,
                new_qty: 3,
            },
        );
        assert_eq!(modified.symbol(), Some(Symbol::BtcUsdt));
        assert_eq!(modified.sequence(), 4);

        let rejected = OrderBookEvent::OrderRejected(
            5,
            Symbol::XrpUsdt,
            OrderRejectedEvent {
                order_id: 5,
                user_id: 105,
                reason: RejectReason::InsufficientBalance,
            },
        );
        assert_eq!(rejected.symbol(), Some(Symbol::XrpUsdt));
        assert_eq!(rejected.sequence(), 5);

        let mod_rejected = OrderBookEvent::ModifyOrderRejected(
            6,
            Symbol::BnbUsdt,
            ModifyOrderRejectedEvent {
                order_id: 6,
                user_id: 106,
                side: Side::Sell,
                old_price: 300,
                new_price: 310,
                old_qty: 1,
                new_qty: 2,
                reason: RejectReason::AccountFrozen,
            },
        );
        assert_eq!(mod_rejected.symbol(), Some(Symbol::BnbUsdt));
        assert_eq!(mod_rejected.sequence(), 6);

        let expired = OrderBookEvent::OrdersExpired(7, Symbol::BtcUsdc, vec![(7, 107, 40_000, Side::Buy, 1), (8, 108, 41_000, Side::Buy, 2)]);
        assert_eq!(expired.symbol(), Some(Symbol::BtcUsdc));
        assert_eq!(expired.sequence(), 7);

        let opened = OrderBookEvent::MarketOpened(8, Symbol::UsdtInr);
        assert_eq!(opened.symbol(), Some(Symbol::UsdtInr));
        assert_eq!(opened.sequence(), 8);

        let closed = OrderBookEvent::MarketClosed(9, Symbol::InrUsdt);
        assert_eq!(closed.symbol(), Some(Symbol::InrUsdt));
        assert_eq!(closed.sequence(), 9);

        let err = OrderBookEvent::Error(10, 404);
        assert_eq!(err.symbol(), None);
        assert_eq!(err.sequence(), 10);
        assert!(err.is_error());
    }

    #[test]
    fn test_event_envelope_constructors() {
        let empty = EventEnvelope::empty();
        assert_eq!(empty.event, None);

        let default_env = EventEnvelope::default();
        assert_eq!(default_env.event, None);

        let event = OrderBookEvent::OrderPlaced(
            1,
            Symbol::BtcInr,
            OrderPlacedEvent {
                order_id: 10,
                user_id: 101,
                side: Side::Buy,
                price: 100,
                quantity: 5,
                order_type: OrderType::GoodTillCancel,
            },
        );
        let with_event = EventEnvelope::new(event.clone());
        assert_eq!(with_event.event, Some(event));
    }

    #[test]
    fn test_event_dispatcher_publish_and_consumer_poll() {
        let (mut dispatcher, mut consumer) = create_event_pipeline(64);

        // Before any publish, poll should return NoEvents
        assert_eq!(consumer.poll().err(), Some(Polling::NoEvents));

        let ev1 = OrderBookEvent::OrderPlaced(
            1,
            Symbol::BtcInr,
            OrderPlacedEvent {
                order_id: 1,
                user_id: 101,
                side: Side::Buy,
                price: 50_000,
                quantity: 2,
                order_type: OrderType::GoodTillCancel,
            },
        );
        let ev2 = OrderBookEvent::TradeExecuted(
            2,
            Symbol::BtcInr,
            TradeExecutedEvent {
                trade_id: 1,
                maker_order_id: 1,
                taker_order_id: 2,
                maker_user_id: 101,
                taker_user_id: 102,
                maker_side: Side::Buy,
                taker_side: Side::Sell,
                price: 50_000,
                quantity: 2,
                maker_remaining_qty: 0,
                taker_remaining_qty: 0,
            },
        );

        // Publish events
        dispatcher.publish(ev1.clone());
        dispatcher.publish(ev2.clone());

        let events = consumer.poll().expect("should successfully poll events");
        assert_eq!(events, vec![ev1, ev2]);
    }

    #[test]
    fn test_event_dispatcher_try_send_success() {
        let (mut dispatcher, mut consumer) = create_event_pipeline(64);

        let ev = OrderBookEvent::OrderCancelled(
            1,
            Symbol::EthInr,
            OrderCancelledEvent {
                order_id: 42,
                user_id: 142,
                side: Side::Sell,
                price: 2_000,
                cancelled_qty: 1,
                reason: CancelReason::UserRequested,
            },
        );
        let res = dispatcher.try_send(ev.clone());
        assert!(res.is_ok());

        let events = consumer.poll().expect("should poll event");
        assert_eq!(events, vec![ev]);
    }

    #[test]
    fn test_event_dispatcher_try_send_buffer_full() {
        let (mut dispatcher, _consumer) = create_event_pipeline(8);

        // Fill ring buffer completely without polling
        for i in 0..8 {
            let res = dispatcher.try_send(OrderBookEvent::OrderPlaced(
                i,
                Symbol::BtcInr,
                OrderPlacedEvent {
                    order_id: i,
                    user_id: 1000 + i,
                    side: Side::Buy,
                    price: 50_000,
                    quantity: 1,
                    order_type: OrderType::GoodTillCancel,
                },
            ));
            assert!(res.is_ok());
        }

        // 9th send must fail with RingBufferFull
        let overflow = dispatcher.try_send(OrderBookEvent::OrderPlaced(
            8,
            Symbol::BtcInr,
            OrderPlacedEvent {
                order_id: 999,
                user_id: 1999,
                side: Side::Buy,
                price: 50_000,
                quantity: 1,
                order_type: OrderType::GoodTillCancel,
            },
        ));
        assert_eq!(overflow, Err(disruptor::RingBufferFull));
    }

    #[test]
    fn test_to_log_data() {
        let placed = OrderBookEvent::OrderPlaced(
            1,
            Symbol::BtcInr,
            OrderPlacedEvent {
                order_id: 10,
                user_id: 101,
                side: Side::Buy,
                price: 50_000,
                quantity: 2,
                order_type: OrderType::GoodTillCancel,
            },
        );
        let log = placed.to_log_data().unwrap();
        assert_eq!(log.event_type.as_deref(), Some("OrderPlaced"));
        assert_eq!(log.sequence_no, Some(1));
        assert_eq!(log.symbol, Some(Symbol::BtcInr));
        assert_eq!(log.order_id, Some(10));
        assert_eq!(log.price, Some(50_000));
        assert_eq!(log.quantity, Some(2));
        assert_eq!(log.order_side, Some(Side::Buy));
        assert_eq!(log.order_type, Some(OrderType::GoodTillCancel));

        let trade = OrderBookEvent::TradeExecuted(
            2,
            Symbol::BtcInr,
            TradeExecutedEvent {
                trade_id: 1,
                maker_order_id: 10,
                taker_order_id: 20,
                maker_user_id: 101,
                taker_user_id: 102,
                maker_side: Side::Buy,
                taker_side: Side::Sell,
                price: 50_000,
                quantity: 1,
                maker_remaining_qty: 1,
                taker_remaining_qty: 0,
            },
        );
        let log = trade.to_log_data().unwrap();
        assert_eq!(log.event_type.as_deref(), Some("TradeExecuted"));
        assert_eq!(log.sequence_no, Some(2));
        assert_eq!(log.symbol, Some(Symbol::BtcInr));
        assert_eq!(log.order_id, Some(10));
        assert_eq!(log.price, Some(50_000));
        assert_eq!(log.quantity, Some(1));
        assert_eq!(log.order_side, Some(Side::Sell));

        let cancelled = OrderBookEvent::OrderCancelled(
            3,
            Symbol::EthInr,
            OrderCancelledEvent {
                order_id: 15,
                user_id: 103,
                side: Side::Sell,
                price: 3_000,
                cancelled_qty: 5,
                reason: CancelReason::UserRequested,
            },
        );
        let log = cancelled.to_log_data().unwrap();
        assert_eq!(log.event_type.as_deref(), Some("OrderCancelled"));
        assert_eq!(log.order_id, Some(15));
        assert_eq!(log.price, Some(3_000));
        assert_eq!(log.quantity, Some(5));
        assert_eq!(log.order_side, Some(Side::Sell));

        let modified = OrderBookEvent::OrderModified(
            4,
            Symbol::BtcUsdt,
            OrderModifiedEvent {
                order_id: 20,
                user_id: 104,
                side: Side::Buy,
                old_price: 50_000,
                new_price: 51_000,
                old_qty: 2,
                new_qty: 3,
            },
        );
        let log = modified.to_log_data().unwrap();
        assert_eq!(log.event_type.as_deref(), Some("OrderModified"));
        assert_eq!(log.order_id, Some(20));
        assert_eq!(log.price, Some(51_000));
        assert_eq!(log.quantity, Some(3));
        assert_eq!(log.order_side, Some(Side::Buy));

        let rejected = OrderBookEvent::OrderRejected(
            5,
            Symbol::XrpUsdt,
            OrderRejectedEvent {
                order_id: 25,
                user_id: 105,
                reason: RejectReason::InsufficientBalance,
            },
        );
        let log = rejected.to_log_data().unwrap();
        assert_eq!(log.event_type.as_deref(), Some("OrderRejected"));
        assert_eq!(log.order_id, Some(25));

        let mod_rejected = OrderBookEvent::ModifyOrderRejected(
            6,
            Symbol::BnbUsdt,
            ModifyOrderRejectedEvent {
                order_id: 30,
                user_id: 106,
                side: Side::Sell,
                old_price: 300,
                new_price: 310,
                old_qty: 1,
                new_qty: 2,
                reason: RejectReason::AccountFrozen,
            },
        );
        let log = mod_rejected.to_log_data().unwrap();
        assert_eq!(log.event_type.as_deref(), Some("ModifyOrderRejected"));
        assert_eq!(log.order_id, Some(30));
        assert_eq!(log.price, Some(310));
        assert_eq!(log.quantity, Some(2));
        assert_eq!(log.order_side, Some(Side::Sell));

        let expired = OrderBookEvent::OrdersExpired(7, Symbol::BtcUsdc, vec![(100, 1, 10, Side::Buy, 1), (101, 2, 20, Side::Buy, 2)]);
        let log = expired.to_log_data().unwrap();
        assert_eq!(log.event_type.as_deref(), Some("OrdersExpired"));
        assert_eq!(log.order_ids, Some(vec![100, 101]));

        let opened = OrderBookEvent::MarketOpened(8, Symbol::UsdtInr);
        assert!(opened.to_log_data().is_none());

        let closed = OrderBookEvent::MarketClosed(9, Symbol::InrUsdt);
        assert!(closed.to_log_data().is_none());

        let error = OrderBookEvent::Error(10, 404);
        assert!(error.to_log_data().is_none());
    }
}
