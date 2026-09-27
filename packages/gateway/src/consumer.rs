use domain::Symbol;
use engine::events::OrderbookEventLog;
use tokio::sync::mpsc::Receiver;

pub async fn spwan_consumer_thread(
    mut reciver: Receiver<OrderbookEventLog>, /* some broadcast of stremaing to web socket fucntions or just call
                                               * this there */
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            let result = reciver.recv().await;
            match result {
                Some(event) => match event.symbol.unwrap_or_else(|| Symbol::Unknown) {
                    Symbol::BnbUsdt => {
                        // broadcast.send(log) // serelize it in json or binary
                        // ? protobuf ? i dont know
                    },
                    Symbol::BtcInr => {},
                    Symbol::BtcUsdc => {},
                    Symbol::BtcUsdt => {},
                    Symbol::EthInr => {},
                    Symbol::EthUsdc => {},
                    Symbol::EthUsdt => {},
                    Symbol::InrUsdt => {},
                    Symbol::SolInr => {},
                    Symbol::SolUsdt => {},
                    Symbol::UsdtInr => {},
                    Symbol::XrpUsdt => {},
                    Symbol::Unknown => {},
                },
                None => todo!(),
            }
        }
    })
}
