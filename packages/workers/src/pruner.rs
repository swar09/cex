use std::{thread, time::Duration};

use chrono::{Duration as ChronoDuration, Utc};
use domain::{OrderType, Symbol};
use engine::commands::{CommandDispatcher, ExchangeCommand};

pub fn spawn_gfd_prune_worker(mut cmd_tx: CommandDispatcher, symbol: Symbol, utc_end_hr: u32) -> thread::JoinHandle<()> {
    thread::Builder::new()
        .name(format!("gfd-pruner-{:?}", symbol))
        .spawn(move || {
            loop {
                let now = Utc::now();

                let target_today = now.date_naive().and_hms_opt(utc_end_hr, 0, 0).map(|naive| naive.and_utc());

                let next_run = match target_today {
                    Some(target) if now < target => target,
                    Some(target) => target + ChronoDuration::days(1),
                    None => {
                        eprintln!("[Pruner-{:?}] Invalid UTC hour: {}", symbol, utc_end_hr);
                        return;
                    },
                };

                if let Ok(std_duration) = (next_run - now).to_std() {
                    thread::sleep(std_duration + Duration::from_millis(100));
                }

                let cmd = ExchangeCommand::PruneExpiredOrders(symbol, OrderType::GoodForDay);

                cmd_tx.publish(cmd);
            }
        })
        .expect("failed to spawn GFD pruning worker")
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use disruptor::{BusySpin, build_single_producer};
    use domain::{NewOrder, Side};
    use engine::{
        events::{EventEnvelope, OrderBookEvent},
        exchange::Exchange,
    };

    use super::*;
    use crate::wal_logger::{WalConfig, build_gated_ingress_pipeline};

    struct TempDirGuard {
        path: PathBuf,
    }

    impl TempDirGuard {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!("cex_pruner_test_{}_{}", name, std::process::id()));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).unwrap();
            Self { path }
        }
    }

    impl Drop for TempDirGuard {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn test_spawn_gfd_prune_worker_spawns_and_handles_invalid_hour() {
        let event_factory = || EventEnvelope { event: None };
        let egress_builder = build_single_producer(64, event_factory, BusySpin).with_multi_consumer();
        let (_egress_poller, egress_builder) = egress_builder.new_event_poller();
        let egress_producer = egress_builder.build();

        let mut exchange = Exchange::new(egress_producer);
        exchange.add_new_orderbook(Symbol::BtcInr);

        let temp = TempDirGuard::new("invalid_hour");
        let wal_config = WalConfig {
            wal_dir: temp.path.clone(),
            segment_size: 4096,
            flush_interval: Duration::from_millis(5),
        };

        let dispatcher = build_gated_ingress_pipeline(64, wal_config, exchange);
        let handle = spawn_gfd_prune_worker(dispatcher, Symbol::BtcInr, 25);
        assert_eq!(handle.thread().name(), Some("gfd-pruner-BtcInr"));
        handle.join().expect("worker should exit on invalid hour");
    }

    #[test]
    fn test_spawn_gfd_prune_worker_spawns_thread() {
        let event_factory = || EventEnvelope { event: None };
        let egress_builder = build_single_producer(64, event_factory, BusySpin).with_multi_consumer();
        let (_egress_poller, egress_builder) = egress_builder.new_event_poller();
        let egress_producer = egress_builder.build();

        let mut exchange = Exchange::new(egress_producer);
        exchange.add_new_orderbook(Symbol::BtcInr);

        let temp = TempDirGuard::new("valid_spawn");
        let wal_config = WalConfig {
            wal_dir: temp.path.clone(),
            segment_size: 4096,
            flush_interval: Duration::from_millis(5),
        };

        let dispatcher = build_gated_ingress_pipeline(64, wal_config, exchange);
        let handle = spawn_gfd_prune_worker(dispatcher, Symbol::BtcInr, 0);
        assert_eq!(handle.thread().name(), Some("gfd-pruner-BtcInr"));
        drop(handle);
    }

    #[test]
    fn test_prune_expired_orders_command_on_disruptor_pipeline() {
        let temp = TempDirGuard::new("prune_pipeline");
        let wal_config = WalConfig {
            wal_dir: temp.path.clone(),
            segment_size: 16384,
            flush_interval: Duration::from_millis(5),
        };

        let event_factory = || EventEnvelope { event: None };
        let egress_builder = build_single_producer(1024, event_factory, BusySpin).with_multi_consumer();
        let (mut egress_poller, egress_builder) = egress_builder.new_event_poller();
        let egress_producer = egress_builder.build();

        let mut exchange = Exchange::new(egress_producer);
        exchange.add_new_orderbook(Symbol::BtcInr);

        let mut holdings = risk::risk_engine::Holdings::default();
        holdings.credit_asset(Symbol::BtcInr.get_quantity_unit().asset_id(), 1_000_000);
        exchange.get_risk_engine_mut().add_account(1, 1_000_000_000, holdings);

        let mut dispatcher = build_gated_ingress_pipeline(1024, wal_config, exchange);

        let gfd_order = NewOrder {
            order_id: 1,
            user_id: 1,
            asset_id: Symbol::BtcInr.get_quantity_unit().asset_id(),
            order_type: OrderType::GoodForDay,
            side: Side::Buy,
            price: Some(100),
            quantity: 1,
        };
        let gtc_order = NewOrder {
            order_id: 2,
            user_id: 1,
            asset_id: Symbol::BtcInr.get_quantity_unit().asset_id(),
            order_type: OrderType::GoodTillCancel,
            side: Side::Buy,
            price: Some(90),
            quantity: 1,
        };

        dispatcher.publish(ExchangeCommand::AddNewOrder(Symbol::BtcInr, gfd_order));
        dispatcher.publish(ExchangeCommand::AddNewOrder(Symbol::BtcInr, gtc_order));

        thread::sleep(Duration::from_millis(50));

        dispatcher.publish(ExchangeCommand::PruneExpiredOrders(Symbol::BtcInr, OrderType::GoodForDay));

        thread::sleep(Duration::from_millis(50));

        let mut events = Vec::new();
        while let Ok(mut guard) = egress_poller.poll() {
            for item in &mut guard {
                if let Some(event) = &item.event {
                    events.push(event.clone());
                }
            }
        }

        assert!(events.len() >= 3);
        let mut expired_orders_found = false;
        for ev in &events {
            if let OrderBookEvent::OrdersExpired(seq, sym, expired) = ev {
                assert_eq!(*sym, Symbol::BtcInr);
                assert!(*seq > 0);
                assert_eq!(expired.len(), 1);
                assert_eq!(expired[0].0, 1);
                expired_orders_found = true;
            }
        }
        assert!(expired_orders_found);
    }
}
