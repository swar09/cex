use std::{sync::Arc, thread, time::Duration};

use engine::events::{EventConsumer, OrderbookEventLog};
use tokio::sync::broadcast::Sender;

pub fn orderbook_events_publisher(mut consumer: EventConsumer, sender: Sender<Arc<OrderbookEventLog>>) -> thread::JoinHandle<()> {
    thread::Builder::new()
        .name("orderbook-events-gateway-publisher".to_string())
        .spawn(move || {
            let mut idle_spins: u32 = 0;

            loop {
                match consumer.poll() {
                    Ok(orderbook_events) => {
                        idle_spins = 0;
                        for event in orderbook_events {
                            if let Some(log) = event.to_log_data() {
                                let _ = sender.send(Arc::new(log));
                            }
                        }
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
                    },
                    Err(disruptor::Polling::Shutdown) => {
                        break;
                    },
                }
            }
        })
        .expect("failed to spawn orderbook events gateway publisher")
}

#[cfg(test)]
mod tests {
    use disruptor::{BusySpin, build_single_producer};
    use domain::{OrderPlacedEvent, OrderType, Side, Symbol};
    use engine::events::{EventDispatcher, EventEnvelope, OrderBookEvent, OrderbookEventType};
    use tokio::sync::broadcast;

    use super::*;

    fn create_test_event_pipeline(buffer_size: usize) -> (EventDispatcher, EventConsumer) {
        let builder = build_single_producer(buffer_size, EventEnvelope::empty, BusySpin).with_multi_consumer();
        let (poller, builder) = builder.new_event_poller();
        let producer = builder.build();
        (EventDispatcher::new(producer), EventConsumer::new(poller))
    }

    #[test]
    fn test_gateway_publisher_broadcasts_events() {
        let (mut dispatcher, consumer) = create_test_event_pipeline(64);
        let (sender, mut receiver) = broadcast::channel(16);

        let handle = orderbook_events_publisher(consumer, sender);

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

        dispatcher.publish(OrderBookEvent::MarketOpened(2, Symbol::BtcInr));
        dispatcher.publish(OrderBookEvent::MarketClosed(3, Symbol::BtcInr));
        dispatcher.publish(OrderBookEvent::Error(4, 500));

        let msg1 = receiver.blocking_recv().expect("should receive OrderPlaced");
        assert_eq!(msg1.event_type, Some(OrderbookEventType::OrderPlaced));
        assert_eq!(msg1.sequence_no, Some(1));

        let msg2 = receiver.blocking_recv().expect("should receive MarketOpened");
        assert_eq!(msg2.event_type, Some(OrderbookEventType::MarketOpened));
        assert_eq!(msg2.sequence_no, Some(2));

        let msg3 = receiver.blocking_recv().expect("should receive MarketClosed");
        assert_eq!(msg3.event_type, Some(OrderbookEventType::MarketClosed));
        assert_eq!(msg3.sequence_no, Some(3));

        let msg4 = receiver.blocking_recv().expect("should receive Error");
        assert_eq!(msg4.event_type, Some(OrderbookEventType::Error));
        assert_eq!(msg4.sequence_no, Some(4));
        assert_eq!(msg4.error_code, Some(500));

        drop(dispatcher);
        assert!(handle.join().is_ok());
    }

    #[test]
    fn test_gateway_publisher_no_receivers_does_not_panic() {
        let (mut dispatcher, consumer) = create_test_event_pipeline(16);
        let (sender, _) = broadcast::channel(16);

        let handle = orderbook_events_publisher(consumer, sender);

        dispatcher.publish(OrderBookEvent::MarketOpened(1, Symbol::BtcInr));

        thread::sleep(Duration::from_millis(30));

        drop(dispatcher);
        assert!(handle.join().is_ok());
    }
}
