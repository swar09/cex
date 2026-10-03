use std::{sync::Arc, thread};

use rdkafka::ClientConfig as KafkaClientConfig;
use tokio::sync::broadcast;
use workers::{
    egress_logger::spawn_orderbook_events_logger_from_base_producer, gateway_publisher::orderbook_events_publisher, pruner::spawn_gfd_prune_worker,
    read_replica_syncer::orderbook_read_replica_syncer,
};

use crate::{config::ServerConfig, pipeline::Pipelines};

pub struct WorkerHandles {
    pub ws_publisher: thread::JoinHandle<()>,
    pub kafka_logger: thread::JoinHandle<()>,
    pub replica_syncer: thread::JoinHandle<()>,
    pub pruners: Vec<thread::JoinHandle<()>>,
}

pub fn spawn_all(cfg: &ServerConfig, pipelines: Pipelines, event_tx: broadcast::Sender<Arc<engine::events::OrderbookEventLog>>) -> WorkerHandles {
    let consumer_ws = pipelines.consumer_ws;
    let tx = event_tx.clone();
    let ws_handle = orderbook_events_publisher(consumer_ws, tx);
    pin_if_enabled(cfg.pinning_enabled, cfg.core_ws_publisher);

    let kafka_base_producer: rdkafka::producer::BaseProducer = KafkaClientConfig::new()
        .set("bootstrap.servers", &cfg.kafka_brokers)
        .set("client.id", "orderbook-egress-producer")
        .set("queue.buffering.max.ms", "5")
        .set("batch.size", "65536")
        .set("compression.type", "lz4")
        .set("acks", "all")
        .set("enable.idempotence", "true")
        .set("message.timeout.ms", "5000")
        .create()
        .expect("failed to create kafka producer");

    let kafka_handle = spawn_orderbook_events_logger_from_base_producer(pipelines.consumer_kafka, kafka_base_producer);
    pin_if_enabled(cfg.pinning_enabled, cfg.core_kafka);

    let redis_client = redis::Client::open(cfg.redis_replica_url.as_str()).expect("failed to open redis client for read-replica-syncer");

    let replica_handle = orderbook_read_replica_syncer(pipelines.consumer_replica, redis_client, pipelines.ext_to_int_map)
        .expect("failed to spawn read-replica-syncer");
    pin_if_enabled(cfg.pinning_enabled, cfg.core_replica);

    let mut pruners = Vec::new();
    for symbol in domain::Symbol::ALL {
        let cmd_tx = pipelines.exchange_client.template.clone();
        let handle = spawn_gfd_prune_worker(cmd_tx, symbol, 0);
        pin_if_enabled(cfg.pinning_enabled, cfg.core_pruner);
        pruners.push(handle);
    }

    WorkerHandles {
        ws_publisher: ws_handle,
        kafka_logger: kafka_handle,
        replica_syncer: replica_handle,
        pruners,
    }
}

fn pin_if_enabled(enabled: bool, core_id: usize) {
    if !enabled {
        return;
    }
    if let Some(cores) = core_affinity::get_core_ids() {
        if let Some(core) = cores.get(core_id) {
            core_affinity::set_for_current(*core);
        }
    }
}
