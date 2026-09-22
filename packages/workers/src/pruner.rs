use std::{thread, time::Duration};

use chrono::{Duration as ChronoDuration, Utc};
use crossbeam::channel::Sender;
use domain::{OrderType, Symbol};
use engine::commands::ExchangeCommand;

pub fn spawn_gfd_prune_worker(
    cmd_tx: Sender<ExchangeCommand>,
    symbol: Symbol,
    utc_end_hr: u32,
) -> thread::JoinHandle<()> {
    thread::Builder::new()
        .name(format!("gfd-pruner-{:?}", symbol))
        .spawn(move || {
            loop {
                let now = Utc::now();

                let target_today = now
                    .date_naive()
                    .and_hms_opt(utc_end_hr, 0, 0)
                    .map(|naive| naive.and_utc());

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

                if let Err(e) = cmd_tx.send(cmd) {
                    eprintln!("[Pruner-{:?}] Engine receiver dropped, stopping worker: {e}", symbol);
                    break;
                }
            }
        })
        .expect("failed to spawn GFD pruning worker")
}
