use std::thread;

use engine::events::{EventConsumer, OrderbookEventLog};
use tokio::sync::mpsc::Sender; // no dedicated spsc option
pub fn orderbook_events_publisher(
    mut consumer: EventConsumer,
    sender: Sender<OrderbookEventLog>,
) -> thread::JoinHandle<()> {
    thread::Builder::new()
        .name("orderbook-events-gateway-publisher".to_string())
        .spawn(move || {
            loop {
                let result = consumer.poll();
                match result {
                    Ok(orderbook_events) => {
                        for event in orderbook_events {
                            let _symbol = event.symbol().unwrap().as_str(); // handle error and publish to unknown symbol
                            let payload = event.to_log_data().unwrap(); // handle error 
                            // blocking send blocks this thread, its a bounded channel
                            match sender.blocking_send(payload) {
                                Ok(_) => {},
                                Err(e) => {
                                    eprintln!("[orderbook-events-gateway-publisher] ERROR : {e}")
                                },
                            }
                        }
                    },
                    Err(e) => {
                        eprintln!("[orderbook-events_gateway-publisher] ERROR : {e}")
                    },
                }
            }
        })
        .expect("failed to spawn orderbook events gateway publisher")
}
