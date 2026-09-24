use std::thread;

use engine::events::{EventConsumer, OrderbookEventLog};
use tokio::sync::mpsc::Sender;
pub fn orderbook_events_publisher(
    mut consumer: EventConsumer,
    sender: Sender<OrderbookEventLog>,
) -> thread::JoinHandle<()> {
    thread::Builder::new()
        .name("orderbook-events-logger".to_string())
        .spawn(move || {
            loop {
                let result = consumer.poll();
                match result {
                    Ok(orderbook_events) => {
                        for event in orderbook_events {
                            let symbol = event.symbol().unwrap().as_str(); // handle error and publish to unknown symbol
                            // let seq = event.
                            // let event_type = event
                            let payload = OrderbookEventLog {
                                event_type: None,
                                sequence_no: None,
                                symbol: Some(String::from(symbol)),
                                order_id: None,
                                price: None,
                                quantity: None,
                                order_ids: None,
                                error_code: None,
                            };
                            match sender.blocking_send(payload) {
                                Ok(_) => {},
                                Err(e) => {
                                    eprintln!("[orderbook_events_gateway_publisher] ERROR : {e}")
                                },
                            }
                        }
                    },
                    Err(e) => {
                        eprintln!("[orderbook_events_gateway_publisher] ERROR : {e}")
                    },
                }
            }
        })
        .expect("failed to spawn orderbook events gateway publisher")
}
