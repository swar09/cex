use std::time::Duration;

use rdkafka::{
    ClientConfig,
    error::KafkaError,
    producer::{BaseProducer, BaseRecord, Producer},
    types::RDKafkaErrorCode,
};

pub const ORDERBOOK_EVENTS_TOPIC: &str = "orderbook.events.logs";
pub const ORDERBOOK_UNKNOWN_TOPIC: &str = "orderbook.events.unknown";
pub const ORDERS_INGRESS_TOPIC: &str = "orders.ingress";

pub const DEFAULT_EGRESS_PRODUCER_NAME: &str = "orderbook-egress-producer";

pub struct OrderbookEventsProducer {
    pub producer: BaseProducer,
    pub default_topic: String,
    pub unknown_topic: String,
}

impl OrderbookEventsProducer {
    pub fn new(brokers: &str, client_id: Option<&str>) -> Result<Self, KafkaError> {
        let producer: BaseProducer = ClientConfig::new()
            .set("bootstrap.servers", brokers)
            .set("client.id", client_id.unwrap_or(DEFAULT_EGRESS_PRODUCER_NAME))
            .set("queue.buffering.max.ms", "5")
            .set("batch.size", "65536")
            .set("compression.type", "lz4")
            .set("acks", "all")
            .set("enable.idempotence", "true")
            .set("message.timeout.ms", "5000")
            .create()?;

        Ok(Self {
            producer,
            default_topic: ORDERBOOK_EVENTS_TOPIC.to_string(),
            unknown_topic: ORDERBOOK_UNKNOWN_TOPIC.to_string(),
        })
    }

    pub fn from_producer(producer: BaseProducer, default_topic: Option<String>, unknown_topic: Option<String>) -> Self {
        Self {
            producer,
            default_topic: default_topic.unwrap_or_else(|| ORDERBOOK_EVENTS_TOPIC.to_string()),
            unknown_topic: unknown_topic.unwrap_or_else(|| ORDERBOOK_UNKNOWN_TOPIC.to_string()),
        }
    }

    pub fn with_topics(mut self, default_topic: impl Into<String>, unknown_topic: impl Into<String>) -> Self {
        self.default_topic = default_topic.into();
        self.unknown_topic = unknown_topic.into();
        self
    }

    pub fn send_event(&self, symbol: Option<&str>, payload: &[u8]) -> Result<(), KafkaError> {
        let (topic, key) = match symbol {
            Some(sym) => (self.default_topic.as_str(), sym),
            None => (self.unknown_topic.as_str(), "UNKNOWN"),
        };

        let record = BaseRecord::to(topic).key(key).payload(payload);
        match self.producer.send(record) {
            Ok(()) => Ok(()),
            Err((KafkaError::MessageProduction(RDKafkaErrorCode::QueueFull), record)) => {
                self.producer.poll(Duration::from_millis(10));
                self.producer.send(record).map_err(|(err, _)| err)
            },
            Err((err, _)) => Err(err),
        }
    }

    pub fn flush(&self, timeout: Duration) -> Result<(), KafkaError> {
        match self.producer.flush(timeout) {
            Ok(()) => Ok(()),
            Err(e) => Err(e),
        }
    }

    pub fn poll(&self, timeout: Duration) {
        self.producer.poll(timeout);
    }
}

pub struct KafkaClient {
    pub producer: BaseProducer,
    pub config: ClientConfig,
}

impl KafkaClient {
    pub fn new(config: ClientConfig) -> Self {
        let producer: BaseProducer = config.create().expect("Cannot create producer");
        Self { producer, config }
    }

    pub fn new_producer(&self) -> Result<BaseProducer, KafkaError> {
        self.config.create()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_orderbook_events_producer_mock_send() {
        let producer: BaseProducer = ClientConfig::new()
            .set("test.mock.num.brokers", "3")
            .create()
            .expect("should create mock producer");

        let egress_producer = OrderbookEventsProducer::from_producer(producer, None, None);

        let res = egress_producer.send_event(Some("BTC_USDT"), b"{\"seq\":1}");
        assert!(res.is_ok());

        let res_unknown = egress_producer.send_event(None, b"{\"error\":404}");
        assert!(res_unknown.is_ok());

        assert!(egress_producer.flush(Duration::from_millis(500)).is_ok());
    }
}
