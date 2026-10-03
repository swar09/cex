use std::{thread, time::Duration};

use engine::events::EventConsumer;
use queue::OrderbookEventsProducer;
use rdkafka::producer::BaseProducer;

pub fn orderbook_events_logger(mut consumer: EventConsumer, producer: OrderbookEventsProducer) -> thread::JoinHandle<()> {
    thread::Builder::new()
        .name("orderbook-egress-publisher".to_string())
        .spawn(move || {
            let mut idle_spins: u32 = 0;

            loop {
                match consumer.poll() {
                    Ok(orderbook_events) => {
                        idle_spins = 0;
                        for event in orderbook_events {
                            let symbol_str = event.symbol().map(|s| s.as_str());

                            let Some(log_data) = event.to_log_data() else {
                                continue;
                            };

                            let payload_bytes = match serde_json::to_vec(&log_data) {
                                Ok(b) => b,
                                Err(e) => {
                                    eprintln!("[orderbook-egress-publisher] serialize log_data error: {e}");
                                    continue;
                                },
                            };

                            if let Err(e) = producer.send_event(symbol_str, &payload_bytes) {
                                eprintln!("[orderbook-egress-publisher] kafka produce error: {e}");
                            }
                        }
                        producer.poll(Duration::from_millis(0));
                    },
                    Err(disruptor::Polling::NoEvents) => {
                        if idle_spins < 100 {
                            std::hint::spin_loop();
                        } else if idle_spins < 500 {
                            thread::yield_now();
                        } else {
                            thread::sleep(Duration::from_micros(50));
                        }
                        idle_spins = idle_spins.saturating_add(1);
                        producer.poll(Duration::from_millis(0));
                    },
                    Err(disruptor::Polling::Shutdown) => {
                        if let Err(e) = producer.flush(Duration::from_secs(1)) {
                            eprintln!("[orderbook-egress-publisher] flush on shutdown error: {e}");
                        }
                        break;
                    },
                }
            }
        })
        .expect("failed to spawn orderbook egress worker")
}

pub fn spawn_orderbook_events_logger_from_base_producer(consumer: EventConsumer, producer: BaseProducer) -> thread::JoinHandle<()> {
    let egress_producer = OrderbookEventsProducer::from_producer(producer, None, None);
    orderbook_events_logger(consumer, egress_producer)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use disruptor::{BusySpin, build_single_producer};
    use domain::{
        CancelReason, ModifyOrderRejectedEvent, OrderCancelledEvent, OrderModifiedEvent, OrderPlacedEvent, OrderRejectedEvent, OrderType,
        RejectReason, Side, Symbol, TradeExecutedEvent,
    };
    use engine::events::{EventDispatcher, EventEnvelope, OrderBookEvent};
    use rdkafka::ClientConfig;

    use super::*;

    fn create_test_event_pipeline(buffer_size: usize) -> (EventDispatcher, EventConsumer) {
        let builder = build_single_producer(buffer_size, EventEnvelope::empty, BusySpin).with_multi_consumer();
        let (poller, builder) = builder.new_event_poller();
        let producer = builder.build();
        (EventDispatcher::new(producer), EventConsumer::new(poller))
    }

    #[test]
    fn test_orderbook_events_logger_publishes_all_event_types() {
        let (mut dispatcher, consumer) = create_test_event_pipeline(64);

        let mock_producer: BaseProducer = ClientConfig::new()
            .set("test.mock.num.brokers", "3")
            .create()
            .expect("failed to create mock kafka producer");

        let egress_producer = OrderbookEventsProducer::from_producer(mock_producer, None, None);
        let worker_handle = orderbook_events_logger(consumer, egress_producer);

        dispatcher.publish(OrderBookEvent::OrderPlaced(
            1,
            Symbol::BtcInr,
            OrderPlacedEvent {
                order_id: 101,
                user_id: 1,
                side: Side::Buy,
                price: 50_000,
                quantity: 2,
                order_type: OrderType::GoodTillCancel,
            },
        ));

        dispatcher.publish(OrderBookEvent::TradeExecuted(
            2,
            Symbol::BtcInr,
            TradeExecutedEvent {
                trade_id: 1,
                maker_order_id: 101,
                taker_order_id: 102,
                maker_user_id: 1,
                taker_user_id: 2,
                maker_side: Side::Buy,
                taker_side: Side::Sell,
                price: 50_000,
                quantity: 1,
                maker_remaining_qty: 1,
                taker_remaining_qty: 0,
            },
        ));

        dispatcher.publish(OrderBookEvent::OrderCancelled(
            3,
            Symbol::EthInr,
            OrderCancelledEvent {
                order_id: 201,
                user_id: 3,
                side: Side::Sell,
                price: 3_000,
                cancelled_qty: 1,
                reason: CancelReason::UserRequested,
            },
        ));

        dispatcher.publish(OrderBookEvent::OrderModified(
            4,
            Symbol::BtcUsdt,
            OrderModifiedEvent {
                order_id: 301,
                user_id: 4,
                side: Side::Buy,
                old_price: 60_000,
                new_price: 61_000,
                old_qty: 5,
                new_qty: 6,
            },
        ));

        dispatcher.publish(OrderBookEvent::OrderRejected(
            5,
            Symbol::XrpUsdt,
            OrderRejectedEvent {
                order_id: 401,
                user_id: 5,
                reason: RejectReason::InsufficientBalance,
            },
        ));

        dispatcher.publish(OrderBookEvent::ModifyOrderRejected(
            6,
            Symbol::BnbUsdt,
            ModifyOrderRejectedEvent {
                order_id: 501,
                user_id: 6,
                side: Side::Sell,
                old_price: 400,
                new_price: 410,
                old_qty: 2,
                new_qty: 3,
                reason: RejectReason::AccountFrozen,
            },
        ));

        dispatcher.publish(OrderBookEvent::OrdersExpired(7, Symbol::BtcUsdc, vec![(601, 7, 70_000, Side::Buy, 1)]));

        dispatcher.publish(OrderBookEvent::MarketOpened(8, Symbol::UsdtInr));
        dispatcher.publish(OrderBookEvent::MarketClosed(9, Symbol::InrUsdt));
        dispatcher.publish(OrderBookEvent::Error(10, 500));

        thread::sleep(Duration::from_millis(50));

        drop(dispatcher);

        let join_result = worker_handle.join();
        assert!(join_result.is_ok());
    }

    #[test]
    fn test_spawn_orderbook_events_logger_from_base_producer() {
        let (dispatcher, consumer) = create_test_event_pipeline(16);

        let mock_producer: BaseProducer = ClientConfig::new()
            .set("test.mock.num.brokers", "3")
            .create()
            .expect("failed to create mock kafka producer");

        let worker_handle = spawn_orderbook_events_logger_from_base_producer(consumer, mock_producer);

        drop(dispatcher);
        assert!(worker_handle.join().is_ok());
    }
}
