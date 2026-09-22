// ) -> Result<thread::JoinHandle<()>, std::io::Error> {
//     thread::Builder::new()
//         .name(format!("book-events-consumer-{:?}", symbol))
//         .spawn(move || {
//             loop {
//                 for event in &event_rcv {
//                     // TODO : finalize the Arch now , orderbook and exchange
//                     // core engine is almost done
//                 }
//             }
//         })
// }

use std::thread;

use engine::{commands::CommandDispatcher, events::EventConsumer};

pub fn orderbook_events_logger(
    mut consumer: EventConsumer,
    _cmd_producer: CommandDispatcher,
) -> Result<thread::JoinHandle<()>, std::io::Error> {
    thread::Builder::new()
        .name("orderbook-events-logger".to_string())
        .spawn(move || {
            // match the event with symbol
            // publish to kafka topic symbol.orderbook.logs
            loop {
                let result = consumer.poll();
                match result {
                    Ok(orderbook_events) => {
                        for event in orderbook_events {
                            let _symbol = event.symbol().unwrap().as_str(); // handle error and publish to unknown topic
                            // create a struct and serialize it
                        }
                    },
                    Err(_e) => {},
                }
            }
        })
}
