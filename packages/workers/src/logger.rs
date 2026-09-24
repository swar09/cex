use std::thread;

use domain::{OrderId, Price, Quantity, Side};
use engine::events::EventConsumer;
use rdkafka::producer::{BaseProducer, BaseRecord};
use serde::Serialize;
#[derive(Debug, Clone, Serialize)]
pub struct OrderbookEventLog {
    pub event_type: Option<String>,
    pub sequence_no: Option<u64>,
    pub symbol: String,
    pub order_id: Option<OrderId>,
    pub price: Option<Price>,
    pub quantity: Option<Quantity>,
    pub side: Option<Side>,
    pub order_ids: Option<Vec<u32>>,
    pub error_code: Option<u64>,
}
pub fn orderbook_events_logger(mut consumer: EventConsumer, producer: BaseProducer) -> thread::JoinHandle<()> {
    thread::Builder::new()
        .name("orderbook-events-logger".to_string())
        .spawn(move || {
            loop {
                let result = consumer.poll();
                match result {
                    Ok(orderbook_events) => {
                        for event in orderbook_events {
                            let symbol = event.symbol().unwrap().as_str(); // handle error and publish to unknown topic
                            let topic = "orderbook.events.logs".to_string();
                            let payload = event.to_log_data().unwrap(); // handle error 
                            let payload_bytes = match serde_json::to_vec(&payload) {
                                Ok(b) => b,
                                Err(e) => {
                                    eprintln!("[orderbook_events_logger] serialize error: {e}");
                                    continue;
                                },
                            };
                            let record = BaseRecord::to(&topic).key(symbol).payload(&payload_bytes);
                            match producer.send(record) {
                                Ok(_) => {
                                    // no need to print result
                                },
                                Err(e) => {
                                    eprintln!("[orderbook_events_logger] kafka error: {}", e.0);
                                },
                            }
                        }
                    },
                    Err(e) => {
                        eprintln!("[orderbook_events_logger] ERROR : {e}")
                    },
                }
            }
        })
        .expect("failed to spawn orderbook logs worker")
}
