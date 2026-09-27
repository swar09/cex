use std::{sync::Arc, thread};

use engine::events::{EventConsumer, OrderbookEventLog};
use tokio::sync::broadcast::Sender;
pub fn orderbook_events_publisher(
    mut consumer: EventConsumer,
    sender: Sender<Arc<OrderbookEventLog>>,
) -> thread::JoinHandle<()> {
    thread::Builder::new()
        .name("orderbook-events-gateway-publisher".to_string())
        .spawn(move || {
            loop {
                let result = consumer.poll();
                match result {
                    Ok(orderbook_events) => {
                        for event in orderbook_events {
                            let payload = Arc::new(event.to_log_data().unwrap()); // handle error 

                            match sender.send(payload) {
                                Ok(_) => {},
                                Err(e) => {
                                    eprintln!("[orderbook-events-gateway-publisher] ERROR :{e}")
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
