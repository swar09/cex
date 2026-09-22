use std::collections::HashMap;

use disruptor::{MultiConsumerBarrier, SingleProducer};
use domain::{OrderIds, Symbol, Trades};
use thiserror::Error;

use crate::{
    commands::ExchangeCommand,
    events::{EventDispatcher, ExchangeEvents, OrderBookEvents},
    orderbook::OrderBook,
};
#[derive(Error, Debug)]
pub enum ExchangeError {
    #[error("Channel send failed")]
    ExchangeEventTrySendError(disruptor::RingBufferFull),
    #[error("Orderbook access failed")]
    OrderBookAccessError(Symbol),
}
pub struct Exchange {
    pub orderbooks: HashMap<Symbol, OrderBook>,
    pub event_tx: EventDispatcher,
}

impl Exchange {
    pub fn new(p: SingleProducer<ExchangeEvents, MultiConsumerBarrier>) -> Self {
        Self {
            orderbooks: HashMap::new(),        // empty
            event_tx: EventDispatcher::new(p), // sender
        }
    }

    pub fn add_new_orderbook(&mut self, symbol: Symbol) {
        let orderbook = OrderBook::new();
        self.orderbooks.insert(symbol, orderbook);
    }

    pub fn handle_cmd(&mut self, cmd: ExchangeCommand) -> Result<(), ExchangeError> {
        match cmd {
            ExchangeCommand::AddNewOrder(symbol, new_order) => {
                let order_id = new_order.order_id;
                let book = self
                    .orderbooks
                    .get_mut(&symbol)
                    .ok_or(ExchangeError::OrderBookAccessError(symbol))?;
                match book.add_new_order(new_order) {
                    Some(trades) => {
                        self.event_tx
                            .try_send(OrderBookEvents::OrderAdded(symbol, order_id))
                            .map_err(ExchangeError::ExchangeEventTrySendError)?;

                        self.handle_trades(trades, symbol)?;
                        Ok(())
                    },
                    None => {
                        self.event_tx
                            .try_send(OrderBookEvents::OrderRejected(symbol, order_id))
                            .map_err(ExchangeError::ExchangeEventTrySendError)?;
                        Ok(())
                    },
                }
            },
            ExchangeCommand::CancelOrder(symbol, order_id) => {
                let book = self
                    .orderbooks
                    .get_mut(&symbol)
                    .ok_or(ExchangeError::OrderBookAccessError(symbol))?;
                if book.cancel_order(order_id) {
                    self.event_tx
                        .try_send(OrderBookEvents::OrderCancelled(symbol, order_id))
                        .map_err(ExchangeError::ExchangeEventTrySendError)?;
                }
                Ok(())
            },
            ExchangeCommand::ModifyOrder(symbol, modify_order) => {
                let book = self
                    .orderbooks
                    .get_mut(&symbol)
                    .ok_or(ExchangeError::OrderBookAccessError(symbol))?;
                match book.modify_order(modify_order) {
                    Some(trades) => {
                        self.event_tx
                            .try_send(OrderBookEvents::OrderModified(symbol, modify_order))
                            .map_err(ExchangeError::ExchangeEventTrySendError)?;
                        self.handle_trades(trades, symbol)?;
                        Ok(())
                    },
                    None => {
                        self.event_tx
                            .try_send(OrderBookEvents::ModifyOrderRejected(symbol, modify_order))
                            .map_err(ExchangeError::ExchangeEventTrySendError)?;
                        Ok(())
                    },
                }
            },
            ExchangeCommand::PruneExpiredOrders(symbol, prune_order_type) => {
                let book = self
                    .orderbooks
                    .get_mut(&symbol)
                    .ok_or(ExchangeError::OrderBookAccessError(symbol))?;
                let mut order_ids: OrderIds = vec![];
                for order_entry in book.orders.values() {
                    let (order_id, order_type) = {
                        (
                            order_entry.order.borrow().order_id,
                            order_entry.order.borrow().get_order_type(),
                        )
                    };
                    if order_type != prune_order_type {
                        continue;
                    }
                    order_ids.push(order_id);
                }
                book.cancel_orders(order_ids.clone());
                self.event_tx
                    .try_send(OrderBookEvents::OrdersExpired(symbol, order_ids))
                    .map_err(ExchangeError::ExchangeEventTrySendError)?;
                Ok(())
            },
        }
    }
    pub fn handle_trades(&mut self, trades: Trades, symbol: Symbol) -> Result<(), ExchangeError> {
        for trade in trades {
            // ask_trade
            let (order_id, price, quantity) = {
                let order = trade.ask_trade;
                (order.order_id, order.price, order.quantity)
            };
            self.event_tx
                .try_send(OrderBookEvents::OrderMatched(symbol, order_id, price, quantity))
                .map_err(ExchangeError::ExchangeEventTrySendError)?;
            // bid_trade
            let (order_id, price, quantity) = {
                let order = trade.bid_trade;
                (order.order_id, order.price, order.quantity)
            };
            self.event_tx
                .try_send(OrderBookEvents::OrderMatched(symbol, order_id, price, quantity))
                .map_err(ExchangeError::ExchangeEventTrySendError)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use disruptor::{BusySpin, EventPoller, SingleProducerBarrier, build_single_producer};
    use domain::{NewOrder, OrderId, OrderType, Side};

    use super::*;

    type TestEventPoller = EventPoller<ExchangeEvents, SingleProducerBarrier>;

    fn new_exchange() -> (Exchange, TestEventPoller) {
        let event_factory = || ExchangeEvents { event: None };
        let builder = build_single_producer(1024, event_factory, BusySpin).with_multi_consumer();
        let (event_poller, builder) = builder.new_event_poller();
        let p = builder.build();
        (Exchange::new(p), event_poller)
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

    fn drain(poller: &mut TestEventPoller) -> Vec<OrderBookEvents> {
        let mut events = Vec::new();
        while let Ok(mut guard) = poller.poll() {
            for item in &mut guard {
                if let Some(event) = &item.event {
                    events.push(event.clone());
                }
            }
        }
        events
    }

    #[test]
    fn gtc_orders_cross_and_match() {
        const SYMBOL: Symbol = Symbol::BtcInr;
        let (mut exchange, mut poller) = new_exchange();
        exchange.add_new_orderbook(SYMBOL);

        exchange
            .handle_cmd(ExchangeCommand::AddNewOrder(SYMBOL, new_buy_gtc(1)))
            .unwrap();
        exchange
            .handle_cmd(ExchangeCommand::AddNewOrder(SYMBOL, new_sell_gtc(2)))
            .unwrap();

        assert_eq!(
            drain(&mut poller),
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
        let (mut exchange, mut poller) = new_exchange();
        exchange.add_new_orderbook(SYMBOL);

        exchange
            .handle_cmd(ExchangeCommand::AddNewOrder(SYMBOL, new_buy_gtc(1)))
            .unwrap();

        assert_eq!(drain(&mut poller), vec![OrderBookEvents::OrderAdded(SYMBOL, 1)]);
    }

    #[test]
    fn fak_order_matches_immediately_when_crossable() {
        const SYMBOL: Symbol = Symbol::BtcInr;
        let (mut exchange, mut poller) = new_exchange();
        exchange.add_new_orderbook(SYMBOL);

        exchange
            .handle_cmd(ExchangeCommand::AddNewOrder(SYMBOL, new_buy_gtc(1)))
            .unwrap();
        exchange
            .handle_cmd(ExchangeCommand::AddNewOrder(SYMBOL, new_sell_fak(2)))
            .unwrap();

        let events = drain(&mut poller);
        assert!(events.contains(&OrderBookEvents::OrderAdded(SYMBOL, 1)));
        assert!(events.contains(&OrderBookEvents::OrderMatched(SYMBOL, 1, 100, 1)));
        assert!(events.contains(&OrderBookEvents::OrderMatched(SYMBOL, 2, 100, 1)));
    }

    #[test]
    fn fak_order_rejected_when_nothing_to_cross() {
        const SYMBOL: Symbol = Symbol::BtcInr;
        let (mut exchange, mut poller) = new_exchange();
        exchange.add_new_orderbook(SYMBOL);

        exchange
            .handle_cmd(ExchangeCommand::AddNewOrder(SYMBOL, new_buy_fak(1)))
            .unwrap();

        assert_eq!(drain(&mut poller), vec![OrderBookEvents::OrderRejected(SYMBOL, 1)]);
    }

    #[test]
    fn cancel_resting_order_emits_cancelled_event() {
        const SYMBOL: Symbol = Symbol::BtcInr;
        let (mut exchange, mut poller) = new_exchange();
        exchange.add_new_orderbook(SYMBOL);

        exchange
            .handle_cmd(ExchangeCommand::AddNewOrder(SYMBOL, new_buy_gtc(1)))
            .unwrap();
        drain(&mut poller);

        exchange.handle_cmd(ExchangeCommand::CancelOrder(SYMBOL, 1)).unwrap();
        assert_eq!(drain(&mut poller), vec![OrderBookEvents::OrderCancelled(SYMBOL, 1)]);
    }

    #[test]
    fn cancel_nonexistent_order_emits_nothing() {
        const SYMBOL: Symbol = Symbol::BtcInr;
        let (mut exchange, mut poller) = new_exchange();
        exchange.add_new_orderbook(SYMBOL);

        exchange.handle_cmd(ExchangeCommand::CancelOrder(SYMBOL, 999)).unwrap();
        assert_eq!(drain(&mut poller), vec![]); // cancel_order returned false, no event sent
    }

    #[test]
    fn cancel_already_filled_order_does_not_recancel() {
        const SYMBOL: Symbol = Symbol::BtcInr;
        let (mut exchange, mut poller) = new_exchange();
        exchange.add_new_orderbook(SYMBOL);

        exchange
            .handle_cmd(ExchangeCommand::AddNewOrder(SYMBOL, new_buy_gtc(1)))
            .unwrap();
        exchange
            .handle_cmd(ExchangeCommand::AddNewOrder(SYMBOL, new_sell_gtc(2)))
            .unwrap();
        drain(&mut poller);

        exchange.handle_cmd(ExchangeCommand::CancelOrder(SYMBOL, 1)).unwrap();
        assert_eq!(drain(&mut poller), vec![]);
    }

    #[test]
    fn orderbooks_are_isolated_per_symbol() {
        let (mut exchange, mut poller) = new_exchange();
        exchange.add_new_orderbook(Symbol::BtcInr);
        exchange.add_new_orderbook(Symbol::EthInr);

        // a resting buy on BtcInr should NOT match a sell on EthInr
        exchange
            .handle_cmd(ExchangeCommand::AddNewOrder(Symbol::BtcInr, new_buy_gtc(1)))
            .unwrap();
        exchange
            .handle_cmd(ExchangeCommand::AddNewOrder(Symbol::EthInr, new_sell_gtc(2)))
            .unwrap();

        let events = drain(&mut poller);
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
        let (mut exchange, mut poller) = new_exchange();
        let mut id: OrderId = 0;
        for symbol in Symbol::ALL.iter() {
            exchange.add_new_orderbook(*symbol);
            exchange
                .handle_cmd(ExchangeCommand::AddNewOrder(*symbol, new_buy_gtc(id)))
                .unwrap();
            id += 1;
            exchange
                .handle_cmd(ExchangeCommand::AddNewOrder(*symbol, new_sell_gtc(id)))
                .unwrap();
            id += 1;
        }

        let events = drain(&mut poller);
        let matched_count = events
            .iter()
            .filter(|e| matches!(e, OrderBookEvents::OrderMatched(..)))
            .count();

        assert_eq!(matched_count, Symbol::ALL.len() * 2);
    }
}
