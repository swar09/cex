use std::thread;

use crossbeam::channel::{Receiver, Sender};
use domain::Symbol;
use engine::events::OrderBookEvents;

pub fn orderbook_events_consumer(
    event_rcv: Receiver<OrderBookEvents>,
    event_tx: Sender<OrderBookEvents>,
    symbol: Symbol,
) -> Result<thread::JoinHandle<()>, std::io::Error> {
    thread::Builder::new()
        .name(format!("book-events-consumer-{:?}", symbol))
        .spawn(move || {
            loop {
                for event in &event_rcv {
                    // TODO : finalize the Arch now , orderbook and exchange
                    // core engine is almost done
                }
            }
        })
}
