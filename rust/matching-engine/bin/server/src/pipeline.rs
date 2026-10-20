use std::sync::Arc;

use disruptor::{BusySpin, build_single_producer};
use domain::Symbol;
use engine::{
    events::{EventConsumer, EventDispatcher, EventEnvelope},
    exchange::Exchange,
};
use fxhash::FxHashMap;
use gateway::exchange::ExchangeClient;
use risk::risk_engine::RiskEngine;
use tokio::sync::broadcast;
use workers::wal_logger::{WalConfig, build_gated_ingress_pipeline};

use crate::config::ServerConfig;

pub struct Pipelines {
    pub exchange_client: ExchangeClient,
    pub event_tx: broadcast::Sender<Arc<engine::events::OrderbookEventLog>>,
    pub consumer_ws: EventConsumer,
    pub consumer_kafka: EventConsumer,
    pub consumer_replica: EventConsumer,
    pub ext_to_int_map: FxHashMap<usize, usize>,
}

pub fn build(cfg: &ServerConfig) -> Pipelines {
    let egress_size = cfg.egress_buffer_size;

    let egress_builder = build_single_producer(egress_size, EventEnvelope::empty, BusySpin).with_multi_consumer();

    let (poller_ws, egress_builder) = egress_builder.new_event_poller();
    let (poller_kafka, egress_builder) = egress_builder.new_event_poller();
    let (poller_replica, egress_builder) = egress_builder.new_event_poller();
    let egress_producer = egress_builder.build();

    let dispatcher = EventDispatcher::new(egress_producer);
    let mut exchange = Exchange::new_with_risk_engine(dispatcher.producer, RiskEngine::new_empty());

    for symbol in Symbol::ALL {
        exchange.add_new_orderbook(symbol);
    }

    let ext_to_int_map: FxHashMap<usize, usize> = FxHashMap::default();

    let wal_config = WalConfig {
        wal_dir: cfg.wal_dir.clone(),
        segment_size: cfg.wal_segment_size,
        flush_interval: cfg.wal_flush_interval,
    };

    let cmd_dispatcher = build_gated_ingress_pipeline(cfg.ingress_buffer_size, wal_config, exchange);
    let exchange_client = ExchangeClient::new(cmd_dispatcher);

    let (event_tx, _) = broadcast::channel(cfg.ws_broadcast_capacity);

    Pipelines {
        exchange_client,
        event_tx,
        consumer_ws: EventConsumer::new(poller_ws),
        consumer_kafka: EventConsumer::new(poller_kafka),
        consumer_replica: EventConsumer::new(poller_replica),
        ext_to_int_map,
    }
}
