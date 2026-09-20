use std::collections::HashMap;

use crossbeam::channel::Sender;
use domain::{Currency, Symbol, Trades};

use crate::{commands::ExchangeCommand, events::OrderBookEvents, orderbook::OrderBook};

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
        self.orderbooks.insert(symbol, orderbook);
    }

    pub fn get_quantity_unit(&self, symbol: Symbol) -> Currency {
        match symbol {
            Symbol::BnbUsdt => Currency::Bnb,
            Symbol::BtcInr => Currency::Btc,
            Symbol::BtcUsdc => Currency::Btc,
            Symbol::BtcUsdt => Currency::Btc,
            Symbol::EthInr => Currency::Eth,
            Symbol::EthUsdc => Currency::Eth,
            Symbol::EthUsdt => Currency::Eth,
            Symbol::InrUsdt => Currency::Inr,
            Symbol::SolInr => Currency::Sol,
            Symbol::SolUsdt => Currency::Sol,
            Symbol::UsdtInr => Currency::Usdt,
            Symbol::XrpUsdt => Currency::Xrp,
        }
    }
    pub fn get_price_unit(&self, symbol: Symbol) -> Currency {
        match symbol {
            Symbol::BnbUsdt => Currency::Usdt,
            Symbol::BtcInr => Currency::Inr,
            Symbol::BtcUsdc => Currency::Usdc,
            Symbol::BtcUsdt => Currency::Usdt,
            Symbol::EthInr => Currency::Inr,
            Symbol::EthUsdc => Currency::Usdc,
            Symbol::EthUsdt => Currency::Usdt,
            Symbol::InrUsdt => Currency::Usdt,
            Symbol::SolInr => Currency::Inr,
            Symbol::SolUsdt => Currency::Usdt,
            Symbol::UsdtInr => Currency::Inr,
            Symbol::XrpUsdt => Currency::Usdt,
        }
    }
    pub fn handle_cmd(&mut self, cmd: ExchangeCommand) {
        match cmd {
            ExchangeCommand::AddNewOrder(symbol, new_order) => {
                let order_id = new_order.order_id;
                let book = self.orderbooks.get_mut(&symbol).unwrap();
                match book.add_new_order(new_order) {
                    Some(trades) => {
                        self.event_tx
                            .try_send(OrderBookEvents::OrderAdded(symbol, order_id))
                            .unwrap();

                        self.handle_trades(trades, symbol)
                    },
                    None => {
                        self.event_tx
                            .try_send(OrderBookEvents::OrderRejected(symbol, order_id))
                            .unwrap();
                    },
                };
            },
            ExchangeCommand::CancelOrder(symbol, order_id) => {
                let book = self.orderbooks.get_mut(&symbol).unwrap();
                if book.cancel_order(order_id) {
                    self.event_tx
                        .try_send(OrderBookEvents::OrderCancelled(symbol, order_id))
                        .unwrap();
                }
            },
            ExchangeCommand::ModifyOrder(symbol, modify_order) => {
                let book = self.orderbooks.get_mut(&symbol).unwrap();
                match book.modify_order(modify_order) {
                    Some(trades) => {
                        self.event_tx
                            .try_send(OrderBookEvents::OrderModified(symbol, modify_order))
                            .unwrap();
                        self.handle_trades(trades, symbol)
                    },
                    None => {
                        self.event_tx
                            .try_send(OrderBookEvents::ModifyOrderRejected(symbol, modify_order))
                            .unwrap();
                    },
                };
            },
        }
    }
    pub fn handle_trades(&mut self, trades: Trades, symbol: Symbol) {
        for trade in trades {
            // ask_trade
            let (order_id, price, quantity) = {
                let order = trade.ask_trade;
                (order.order_id, order.price, order.quantity)
            };
            self.event_tx
                .try_send(OrderBookEvents::OrderMatched(symbol, order_id, price, quantity))
                .unwrap();
            // bid_trade
            let (order_id, price, quantity) = {
                let order = trade.bid_trade;
                (order.order_id, order.price, order.quantity)
            };
            self.event_tx
                .try_send(OrderBookEvents::OrderMatched(symbol, order_id, price, quantity))
                .unwrap();
        }
    }
}

#[cfg(test)]
mod tests {
    use crossbeam::channel::{Receiver, bounded};
    use domain::{NewOrder, OrderId, OrderType, Side};

    use super::*;
    fn new_exchange() -> (Exchange, Receiver<OrderBookEvents>) {
        let (s, r) = bounded(100);
        (Exchange::new(s), r)
    }

    fn new_buy_fak(order_id: OrderId) -> NewOrder {
        NewOrder {
            order_type: OrderType::FillAndKill,
            order_id,
            side: Side::Buy,
            price: Some(100),
            quantity: 1,
        }
    }
    fn new_sell_fak(order_id: OrderId) -> NewOrder {
        NewOrder {
            order_type: OrderType::FillAndKill,
            order_id,
            side: Side::Sell,
            price: Some(100),
            quantity: 1,
        }
    }
    fn new_buy_gtc(order_id: OrderId) -> NewOrder {
        NewOrder {
            order_type: OrderType::GoodTillCancel,
            order_id,
            side: Side::Buy,
            price: Some(100),
            quantity: 1,
        }
    }
    fn new_sell_gtc(order_id: OrderId) -> NewOrder {
        NewOrder {
            order_type: OrderType::GoodTillCancel,
            order_id,
            side: Side::Sell,
            price: Some(100),
            quantity: 1,
        }
    }

    fn drain(r: &Receiver<OrderBookEvents>) -> Vec<OrderBookEvents> {
        r.try_iter().collect()
    }

    #[test]
    fn gtc_orders_cross_and_match() {
        const SYMBOL: Symbol = Symbol::BtcInr;
        let (mut exchange, r) = new_exchange();
        exchange.add_new_orderbook(SYMBOL);

        exchange.handle_cmd(ExchangeCommand::AddNewOrder(SYMBOL, new_buy_gtc(1)));
        exchange.handle_cmd(ExchangeCommand::AddNewOrder(SYMBOL, new_sell_gtc(2)));

        assert_eq!(
            drain(&r),
            vec![
                OrderBookEvents::OrderAdded(SYMBOL, 1),
                OrderBookEvents::OrderAdded(SYMBOL, 2),
                OrderBookEvents::OrderMatched(SYMBOL, 2, 100, 1),
                OrderBookEvents::OrderMatched(SYMBOL, 1, 100, 1),
            ]
        );
    }

    #[test]
    fn gtc_order_rests_when_no_counterparty() {
        const SYMBOL: Symbol = Symbol::BtcInr;
        let (mut exchange, r) = new_exchange();
        exchange.add_new_orderbook(SYMBOL);

        exchange.handle_cmd(ExchangeCommand::AddNewOrder(SYMBOL, new_buy_gtc(1)));

        assert_eq!(drain(&r), vec![OrderBookEvents::OrderAdded(SYMBOL, 1)]);
    }

    #[test]
    fn fak_order_matches_immediately_when_crossable() {
        const SYMBOL: Symbol = Symbol::BtcInr;
        let (mut exchange, r) = new_exchange();
        exchange.add_new_orderbook(SYMBOL);

        exchange.handle_cmd(ExchangeCommand::AddNewOrder(SYMBOL, new_buy_gtc(1)));
        exchange.handle_cmd(ExchangeCommand::AddNewOrder(SYMBOL, new_sell_fak(2)));

        let events = drain(&r);
        assert!(events.contains(&OrderBookEvents::OrderAdded(SYMBOL, 1)));
        assert!(events.contains(&OrderBookEvents::OrderMatched(SYMBOL, 1, 100, 1)));
        assert!(events.contains(&OrderBookEvents::OrderMatched(SYMBOL, 2, 100, 1)));
    }

    #[test]
    fn fak_order_rejected_when_nothing_to_cross() {
        const SYMBOL: Symbol = Symbol::BtcInr;
        let (mut exchange, r) = new_exchange();
        exchange.add_new_orderbook(SYMBOL);

        exchange.handle_cmd(ExchangeCommand::AddNewOrder(SYMBOL, new_buy_fak(1)));

        assert_eq!(drain(&r), vec![OrderBookEvents::OrderRejected(SYMBOL, 1)]);
    }

    #[test]
    fn cancel_resting_order_emits_cancelled_event() {
        const SYMBOL: Symbol = Symbol::BtcInr;
        let (mut exchange, r) = new_exchange();
        exchange.add_new_orderbook(SYMBOL);

        exchange.handle_cmd(ExchangeCommand::AddNewOrder(SYMBOL, new_buy_gtc(1)));
        drain(&r);

        exchange.handle_cmd(ExchangeCommand::CancelOrder(SYMBOL, 1));
        assert_eq!(drain(&r), vec![OrderBookEvents::OrderCancelled(SYMBOL, 1)]);
    }

    #[test]
    fn cancel_nonexistent_order_emits_nothing() {
        const SYMBOL: Symbol = Symbol::BtcInr;
        let (mut exchange, r) = new_exchange();
        exchange.add_new_orderbook(SYMBOL);

        exchange.handle_cmd(ExchangeCommand::CancelOrder(SYMBOL, 999));
        assert_eq!(drain(&r), vec![]); // cancel_order returned false, no event sent
    }

    #[test]
    fn cancel_already_filled_order_does_not_recancel() {
        const SYMBOL: Symbol = Symbol::BtcInr;
        let (mut exchange, r) = new_exchange();
        exchange.add_new_orderbook(SYMBOL);

        exchange.handle_cmd(ExchangeCommand::AddNewOrder(SYMBOL, new_buy_gtc(1)));
        exchange.handle_cmd(ExchangeCommand::AddNewOrder(SYMBOL, new_sell_gtc(2)));
        drain(&r);

        exchange.handle_cmd(ExchangeCommand::CancelOrder(SYMBOL, 1));
        assert_eq!(drain(&r), vec![]);
    }

    #[test]
    fn orderbooks_are_isolated_per_symbol() {
        let (mut exchange, r) = new_exchange();
        exchange.add_new_orderbook(Symbol::BtcInr);
        exchange.add_new_orderbook(Symbol::EthInr);

        // a resting buy on BtcInr should NOT match a sell on EthInr
        exchange.handle_cmd(ExchangeCommand::AddNewOrder(Symbol::BtcInr, new_buy_gtc(1)));
        exchange.handle_cmd(ExchangeCommand::AddNewOrder(Symbol::EthInr, new_sell_gtc(2)));

        let events = drain(&r);
        assert_eq!(
            events,
            vec![
                OrderBookEvents::OrderAdded(Symbol::BtcInr, 1),
                OrderBookEvents::OrderAdded(Symbol::EthInr, 2),
            ]
        );
        assert!(!events.iter().any(|e| matches!(e, OrderBookEvents::OrderMatched(..))));
    }

    #[test]
    fn every_symbol_gets_its_own_working_book() {
        let (mut exchange, r) = new_exchange();
        let mut id: OrderId = 0;
        for symbol in Symbol::ALL.iter() {
            exchange.add_new_orderbook(*symbol);
            exchange.handle_cmd(ExchangeCommand::AddNewOrder(*symbol, new_buy_gtc(id)));
            id += 1;
            exchange.handle_cmd(ExchangeCommand::AddNewOrder(*symbol, new_sell_gtc(id)));
            id += 1;
        }

        let events = drain(&r);
        let matched_count = events
            .iter()
            .filter(|e| matches!(e, OrderBookEvents::OrderMatched(..)))
            .count();

        assert_eq!(matched_count, Symbol::ALL.len() * 2);
    }
}
