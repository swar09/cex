use std::collections::HashMap;

use crossbeam::channel::Sender;
use domain::{Currency, Symbol, Trades};

use crate::{
    commands::ExchangeCommand,
    events::OrderBookEvents,
    orderbook::{self, OrderBook},
};

// TODO : fix the unwraps later
pub enum ExchangeError {
    ExchangeEventTrySendError,
}
pub struct Exchange {
    pub orderbooks: HashMap<Symbol, OrderBook>,
    pub event_tx: Sender<OrderBookEvents>,
}

impl Exchange {
    pub fn new(sender: Sender<OrderBookEvents>) -> Self {
        Self {
            orderbooks: HashMap::new(), // empty
            event_tx: sender,           // sender
        }
    }

    pub fn add_new_orderbook(&mut self, symbol: Symbol) {
        let orderbook = OrderBook::new();
        self.orderbooks.insert(symbol, orderbook).unwrap();
    }

    pub fn get_quantity_unit(&self, symbol: Symbol) -> Currency {
        match symbol {
            Symbol::BnbUsdt => return Currency::Bnb,
            Symbol::BtcInr => return Currency::Btc,
            Symbol::BtcUsdc => return Currency::Btc,
            Symbol::BtcUsdt => return Currency::Btc,
            Symbol::EthInr => return Currency::Eth,
            Symbol::EthUsdc => return Currency::Eth,
            Symbol::EthUsdt => return Currency::Eth,
            Symbol::InrUsdt => return Currency::Inr,
            Symbol::SolInr => return Currency::Sol,
            Symbol::SolUsdt => return Currency::Sol,
            Symbol::UsdtInr => return Currency::Usdt,
            Symbol::XrpUsdt => return Currency::Xrp,
        }
    }
    pub fn get_price_unit(&self, symbol: Symbol) -> Currency {
        match symbol {
            Symbol::BnbUsdt => return Currency::Usdt,
            Symbol::BtcInr => return Currency::Inr,
            Symbol::BtcUsdc => return Currency::Usdc,
            Symbol::BtcUsdt => return Currency::Usdt,
            Symbol::EthInr => return Currency::Inr,
            Symbol::EthUsdc => return Currency::Usdc,
            Symbol::EthUsdt => return Currency::Usdt,
            Symbol::InrUsdt => return Currency::Usdt,
            Symbol::SolInr => return Currency::Inr,
            Symbol::SolUsdt => return Currency::Usdt,
            Symbol::UsdtInr => return Currency::Inr,
            Symbol::XrpUsdt => return Currency::Usdt,
        }
    }
    pub fn handle_cmd(&mut self, cmd: ExchangeCommand) {
        match cmd {
            ExchangeCommand::AddNewOrder(symbol, new_order) => {
                let order_id = new_order.order_id;
                let book = self.orderbooks.get_mut(&symbol).unwrap();
                let trades = book.add_new_order(new_order).unwrap();
                self.event_tx.try_send(OrderBookEvents::OrderAdded(order_id)).unwrap();
                self.handle_trades(trades);
            },
            ExchangeCommand::CancelOrder(symbol, order_id) => {
                let book = self.orderbooks.get_mut(&symbol).unwrap();
                if book.cancel_order(order_id) {
                    self.event_tx
                        .try_send(OrderBookEvents::OrderCancelled(order_id))
                        .unwrap();
                }
            },
            ExchangeCommand::ModifyOrder(symbol, modify_order) => {
                let book = self.orderbooks.get_mut(&symbol).unwrap();
                let trades = book.modify_order(modify_order).unwrap();
                self.event_tx
                    .try_send(OrderBookEvents::OrderModified(modify_order))
                    .unwrap();
                self.handle_trades(trades);
            },
        }
    }
    pub fn handle_trades(&mut self, trades: Trades) {
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
    }
}
