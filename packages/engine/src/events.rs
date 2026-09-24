use disruptor::{EventPoller, MultiConsumerBarrier, Polling, Producer, SingleProducer, SingleProducerBarrier};
use domain::{ModifyOrder, OrderId, OrderIds, OrderType, Price, Quantity, Side, Symbol};
use serde::Serialize;

use crate::exchange::Sequence;
#[derive(Debug, Clone, Serialize)]
pub struct OrderbookEventLog {
    pub event_type: Option<String>,
    pub sequence_no: Option<u64>,
    pub symbol: Option<String>,
    pub order_id: Option<OrderId>,
    pub price: Option<Price>,
    pub quantity: Option<Quantity>,
    pub order_type: Option<OrderType>,
    pub order_side: Option<Side>,
    pub order_ids: Option<Vec<u64>>,
    pub error_code: Option<u64>,
}

// orderbook events single producer multiple consumers
#[derive(Clone, PartialEq, Debug)]
pub enum OrderBookEvent {
    OrderMatched(Sequence, Symbol, OrderId, Price, Quantity), // trade occurred
    OrderCancelled(Sequence, Symbol, OrderId),                // order cancelled by trader
    OrderAdded(Sequence, Symbol, OrderId),                    // order added by trader
    OrderModified(Sequence, Symbol, ModifyOrder),             // order modified by trader
    OrderRejected(Sequence, Symbol, OrderId),                 // order rejected by exchange
    ModifyOrderRejected(Sequence, Symbol, ModifyOrder),       // modify order request rejected by exchange
    OrdersExpired(Sequence, Symbol, OrderIds),                // orders expired
    OrderPartiallyFilled(Sequence, Symbol, OrderId),          // order partially filled
    MarketOpened(Sequence, Symbol),                           // market opened
    MarketClosed(Sequence, Symbol),                           // market closed
    Error(Sequence, u32),                                     // error code in orderbook
}

impl OrderBookEvent {
    pub fn symbol(&self) -> Option<Symbol> {
        match self {
            Self::OrderMatched(_, s, ..)
            | Self::OrderCancelled(_, s, ..)
            | Self::OrderAdded(_, s, ..)
            | Self::OrderModified(_, s, ..)
            | Self::OrderRejected(_, s, ..)
            | Self::ModifyOrderRejected(_, s, ..)
            | Self::OrdersExpired(_, s, ..)
            | Self::OrderPartiallyFilled(_, s, ..)
            | Self::MarketOpened(_, s)
            | Self::MarketClosed(_, s) => Some(*s),
            Self::Error(..) => None,
        }
    }

    pub fn to_log_data(&self) -> Option<OrderbookEventLog> {
        match self {
            Self::OrderMatched(seq, symbol, order_id, price, quantity) => {
                let event_type = self.as_str().to_string();
                let symbol = symbol.as_str().to_string();
                Some(OrderbookEventLog {
                    event_type: Some(event_type),
                    sequence_no: Some(*seq),
                    symbol: Some(symbol),
                    order_id: Some(*order_id),
                    price: Some(*price),
                    quantity: Some(*quantity),
                    order_side: None,
                    order_type: None,
                    order_ids: None,
                    error_code: None,
                })
            },

            Self::OrderCancelled(seq, symbol, order_id) => {
                let event_type = self.as_str().to_string();
                let symbol = symbol.as_str().to_string();
                Some(OrderbookEventLog {
                    event_type: Some(event_type),
                    sequence_no: Some(*seq),
                    symbol: Some(symbol),
                    order_id: Some(*order_id),
                    order_type: None,
                    price: None,
                    order_side: None,
                    quantity: None,
                    order_ids: None,
                    error_code: None,
                })
            },

            Self::OrderAdded(seq, symbol, order_id) => {
                let event_type = self.as_str().to_string();
                let symbol = symbol.as_str().to_string();
                Some(OrderbookEventLog {
                    event_type: Some(event_type),
                    sequence_no: Some(*seq),
                    symbol: Some(symbol),
                    order_id: Some(*order_id),
                    price: None,
                    quantity: None,
                    order_side: None,
                    order_ids: None,
                    order_type: None,
                    error_code: None,
                })
            },

            Self::OrderModified(seq, symbol, modify_order) => {
                let event_type = self.as_str().to_string();
                let symbol = symbol.as_str().to_string();

                let order_id = modify_order.get_order_id();
                let price = modify_order.get_price();
                let quantity = modify_order.get_quantity();
                let order_type = modify_order.order_type;
                let side = modify_order.get_side();

                Some(OrderbookEventLog {
                    event_type: Some(event_type),
                    sequence_no: Some(*seq),
                    symbol: Some(symbol),
                    order_id: Some(order_id),
                    price: Some(price),
                    quantity: Some(quantity),
                    order_type: Some(order_type),
                    order_side: Some(side),
                    order_ids: None,
                    error_code: None,
                })
            },

            Self::OrderRejected(seq, symbol, order_id) => {
                let event_type = self.as_str().to_string();
                let symbol = symbol.as_str().to_string();
                Some(OrderbookEventLog {
                    event_type: Some(event_type),
                    sequence_no: Some(*seq),
                    symbol: Some(symbol),
                    order_id: Some(*order_id),
                    order_type: None,
                    price: None,
                    order_side: None,
                    quantity: None,
                    order_ids: None,
                    error_code: None,
                })
            },

            Self::ModifyOrderRejected(seq, symbol, modify_order) => {
                let event_type = self.as_str().to_string();
                let symbol = symbol.as_str().to_string();

                let order_id = modify_order.get_order_id();
                let price = modify_order.get_price();
                let quantity = modify_order.get_quantity();
                let order_type = modify_order.order_type;
                let side = modify_order.get_side();

                Some(OrderbookEventLog {
                    event_type: Some(event_type),
                    sequence_no: Some(*seq),
                    symbol: Some(symbol),
                    order_id: Some(order_id),
                    price: Some(price),
                    quantity: Some(quantity),
                    order_type: Some(order_type),
                    order_side: Some(side),
                    order_ids: None,
                    error_code: None,
                })
            },

            Self::OrdersExpired(seq, symbol, order_ids) => {
                let event_type = self.as_str().to_string();
                let symbol = symbol.as_str().to_string();
                Some(OrderbookEventLog {
                    event_type: Some(event_type),
                    sequence_no: Some(*seq),
                    symbol: Some(symbol),
                    order_id: None,
                    price: None,
                    quantity: None,
                    order_type: None,
                    order_side: None,
                    order_ids: Some(order_ids.to_vec()),
                    error_code: None,
                })
            },

            Self::OrderPartiallyFilled(seq, symbol, order_id) => {
                // TODO
                let event_type = self.as_str().to_string();
                let symbol = symbol.as_str().to_string();
                Some(OrderbookEventLog {
                    event_type: Some(event_type),
                    sequence_no: Some(*seq),
                    symbol: Some(symbol),
                    order_id: Some(*order_id),
                    price: None,
                    quantity: None,
                    order_side: None,
                    order_type: None,
                    order_ids: None,
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
            Self::Error(_seq, _symbol) => {
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
            Self::OrderMatched(..) => "OrderMatched",
            Self::OrderAdded(..) => "OrderAdded",
            Self::OrderCancelled(..) => "OrderCancelled",
            Self::OrderModified(..) => "OrderModified",
            Self::OrderPartiallyFilled(..) => "OrderPartiallyFilled",
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
    use domain::{OrderType, Side};

    use super::*;

    fn create_event_pipeline(buffer_size: usize) -> (EventDispatcher, EventConsumer) {
        let builder = build_single_producer(buffer_size, EventEnvelope::empty, BusySpin).with_multi_consumer();
        let (poller, builder) = builder.new_event_poller();
        let producer = builder.build();
        (EventDispatcher::new(producer), EventConsumer::new(poller))
    }

    #[test]
    fn test_orderbook_event_symbol_extraction() {
        let matched = OrderBookEvent::OrderMatched(1, Symbol::BtcInr, 1, 100, 10);
        assert_eq!(matched.symbol(), Some(Symbol::BtcInr));
        assert!(!matched.is_error());

        let cancelled = OrderBookEvent::OrderCancelled(1, Symbol::EthInr, 2);
        assert_eq!(cancelled.symbol(), Some(Symbol::EthInr));

        let added = OrderBookEvent::OrderAdded(1, Symbol::SolInr, 3);
        assert_eq!(added.symbol(), Some(Symbol::SolInr));

        let modified = OrderBookEvent::OrderModified(
            1,
            Symbol::BtcUsdt,
            ModifyOrder {
                order_type: OrderType::GoodTillCancel,
                order_id: 4,
                side: Side::Buy,
                price: 50_000,
                quantity: 2,
            },
        );
        assert_eq!(modified.symbol(), Some(Symbol::BtcUsdt));

        let rejected = OrderBookEvent::OrderRejected(1, Symbol::XrpUsdt, 5);
        assert_eq!(rejected.symbol(), Some(Symbol::XrpUsdt));

        let mod_rejected = OrderBookEvent::ModifyOrderRejected(
            1,
            Symbol::BnbUsdt,
            ModifyOrder {
                order_type: OrderType::FillAndKill,
                order_id: 6,
                side: Side::Sell,
                price: 300,
                quantity: 1,
            },
        );
        assert_eq!(mod_rejected.symbol(), Some(Symbol::BnbUsdt));

        let expired = OrderBookEvent::OrdersExpired(1, Symbol::BtcUsdc, vec![7, 8]);
        assert_eq!(expired.symbol(), Some(Symbol::BtcUsdc));

        let partial = OrderBookEvent::OrderPartiallyFilled(1, Symbol::EthUsdc , 0);
        assert_eq!(partial.symbol(), Some(Symbol::EthUsdc));

        let opened = OrderBookEvent::MarketOpened(1, Symbol::UsdtInr);
        assert_eq!(opened.symbol(), Some(Symbol::UsdtInr));

        let closed = OrderBookEvent::MarketClosed(1, Symbol::InrUsdt);
        assert_eq!(closed.symbol(), Some(Symbol::InrUsdt));

        let err = OrderBookEvent::Error(1, 404);
        assert_eq!(err.symbol(), None);
        assert!(err.is_error());
    }

    #[test]
    fn test_event_envelope_constructors() {
        let empty = EventEnvelope::empty();
        assert_eq!(empty.event, None);

        let default_env = EventEnvelope::default();
        assert_eq!(default_env.event, None);

        let with_event = EventEnvelope::new(OrderBookEvent::OrderAdded(1, Symbol::BtcInr, 10));
        assert_eq!(
            with_event.event,
            Some(OrderBookEvent::OrderAdded(1, Symbol::BtcInr, 10))
        );
    }

    #[test]
    fn test_event_dispatcher_publish_and_consumer_poll() {
        let (mut dispatcher, mut consumer) = create_event_pipeline(64);

        // Before any publish, poll should return NoEvents
        assert_eq!(consumer.poll().err(), Some(Polling::NoEvents));

        // Publish events
        dispatcher.publish(OrderBookEvent::OrderAdded(1, Symbol::BtcInr, 1));
        dispatcher.publish(OrderBookEvent::OrderMatched(2, Symbol::BtcInr, 1, 50_000, 2));

        let events = consumer.poll().expect(
            "should successfully poll
events",
        );
        assert_eq!(
            events,
            vec![
                OrderBookEvent::OrderAdded(1, Symbol::BtcInr, 1),
                OrderBookEvent::OrderMatched(2, Symbol::BtcInr, 1, 50_000, 2),
            ]
        );
    }

    #[test]
    fn test_event_dispatcher_try_send_success() {
        let (mut dispatcher, mut consumer) = create_event_pipeline(64);

        let res = dispatcher.try_send(OrderBookEvent::OrderCancelled(1, Symbol::EthInr, 42));
        assert!(res.is_ok());

        let events = consumer.poll().expect("should poll event");
        assert_eq!(events, vec![OrderBookEvent::OrderCancelled(1, Symbol::EthInr, 42)]);
    }

    #[test]
    fn test_event_dispatcher_try_send_buffer_full() {
        let (mut dispatcher, _consumer) = create_event_pipeline(8);

        // Fill ring buffer completely without polling
        for i in 0..8 {
            let res = dispatcher.try_send(OrderBookEvent::OrderAdded(i, Symbol::BtcInr, i));
            assert!(res.is_ok());
        }

        // 9th send must fail with RingBufferFull
        let overflow = dispatcher.try_send(OrderBookEvent::OrderAdded(8, Symbol::BtcInr, 999));
        assert_eq!(overflow, Err(disruptor::RingBufferFull));
    }
}
