use std::collections::HashMap;

use crossbeam::channel::Sender;
use domain::Symbol;

use crate::{events::OrderBookEvents, orderbook::OrderBook};

// pub enum ExchangeError {}
pub struct Exchange {
    pub orderbooks: HashMap<Symbol, OrderBook>,
    pub event_tx: Sender<OrderBookEvents>,
}

// impl Exchange {
//     pub fn new() {}
//     pub fn handle_cmd(&mut self, cmd: ExchangeCommand) {
//         match cmd {
//             ExchangeCommand::AddNewOrder(symbol, new_order) => {
//                 let book = self.orderbooks.get_mut(&symbol).unwrap();
//                 let trades = book.add_new_order(new_order).unwrap();
//                 
// self.event_tx.try_send(OrderBookEvents::OrderMatched()).unwrap();            
// },             ExchangeCommand::CancelOrder(Symbol, OrderId) => {},
//             ExchangeCommand::ModifyOrder(Symbol, ModifyOrder) => {},
//         }
//     }
// }
