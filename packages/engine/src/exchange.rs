use std::collections::HashMap;

use crossbeam::channel::Sender;
use domain::Symbol;

use crate::{commands::ExchangeCommand, events::OrderBookEvents, orderbook::OrderBook};

// pub enum ExchangeError {}
pub struct Exchange {
    pub orderbooks: HashMap<Symbol, OrderBook>,
    pub event_tx: Sender<OrderBookEvents>,
}

impl Exchange {
    pub fn new() {}
    pub fn handle_cmd(&mut self, cmd: ExchangeCommand) {
        match cmd {
            ExchangeCommand::AddNewOrder(symbol, new_order) => {
                let book = self.orderbooks.get_mut(&symbol).unwrap();
                let trades = book.add_new_order(new_order).unwrap();
                for trade in trades {
                    let (order_id, price, quantity) = {
                        let order = trade.ask_trade;
                        (order.order_id, order.price, order.quantity)
                    };
                    // ask_trade
                    self.event_tx
                        .try_send(OrderBookEvents::OrderMatched(order_id, price, quantity))
                        .unwrap();
                    // bid_trade
                    let (order_id, price, quantity) = {
                        let order = trade.ask_trade;
                        (order.order_id, order.price, order.quantity)
                    };
                    self.event_tx
                        .try_send(OrderBookEvents::OrderMatched(order_id, price, quantity))
                        .unwrap();
                }
            },
            ExchangeCommand::CancelOrder(symbol, order_id) => {},
            ExchangeCommand::ModifyOrder(symbol, modify_order) => {},
        }
    }
}
