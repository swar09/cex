pub type Sequence = u64;
use disruptor::{MultiConsumerBarrier, SingleProducer};
use domain::{AssetId, Order, OrderIds, Side, Symbol, Trades};
use fxhash::FxHashMap;
use risk::risk_engine::{ExternalUserId, RiskEngine};
use thiserror::Error;

use crate::{
    commands::ExchangeCommand,
    events::{EventDispatcher, EventEnvelope, OrderBookEvent},
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
    pub orderbooks: FxHashMap<Symbol, OrderBook>,
    pub event_tx: EventDispatcher,
    seqs: FxHashMap<Symbol, Sequence>,
    risk_engine: RiskEngine,
}

impl Exchange {
    pub fn new(p: SingleProducer<EventEnvelope, MultiConsumerBarrier>) -> Self {
        Self {
            orderbooks: FxHashMap::default(),  // empty
            event_tx: EventDispatcher::new(p), // sender
            seqs: FxHashMap::default(),
            risk_engine: RiskEngine::new_empty(),
        }
    }

    pub fn new_with_risk_engine(
        p: SingleProducer<EventEnvelope, MultiConsumerBarrier>,
        risk_engine: RiskEngine,
    ) -> Self {
        Self {
            orderbooks: FxHashMap::default(),
            event_tx: EventDispatcher::new(p),
            seqs: FxHashMap::default(),
            risk_engine,
        }
    }

    pub fn get_risk_engine(&self) -> &RiskEngine {
        &self.risk_engine
    }

    pub fn get_risk_engine_mut(&mut self) -> &mut RiskEngine {
        &mut self.risk_engine
    }

    pub fn next_seq(&mut self, symbol: &Symbol) -> Sequence {
        let seq = self.seqs.entry(*symbol).or_insert(0);
        *seq += 1;
        *seq
    }

    pub fn add_new_orderbook(&mut self, symbol: Symbol) {
        let orderbook = OrderBook::new();
        self.orderbooks.insert(symbol, orderbook);
    }

    pub fn handle_cmd(&mut self, cmd: ExchangeCommand) -> Result<(), ExchangeError> {
        match cmd {
            ExchangeCommand::AddNewOrder(symbol, new_order) => {
                let order_id = new_order.order_id;
                let user_id = new_order.user_id as ExternalUserId;
                let asset_id = symbol.get_quantity_unit().asset_id();
                let order = Order::from(new_order.clone());

                if !self.risk_engine.check_and_reserve(user_id, order) {
                    let seq = self.next_seq(&symbol);
                    self.event_tx
                        .try_send(OrderBookEvent::OrderRejected(seq, symbol, order_id))
                        .map_err(ExchangeError::ExchangeEventTrySendError)?;
                    return Ok(());
                }

                let book = self
                    .orderbooks
                    .get_mut(&symbol)
                    .ok_or(ExchangeError::OrderBookAccessError(symbol))?;
                match book.add_new_order(new_order) {
                    Some(trades) => {
                        let seq = self.next_seq(&symbol);
                        self.event_tx
                            .try_send(OrderBookEvent::OrderAdded(seq, symbol, order_id))
                            .map_err(ExchangeError::ExchangeEventTrySendError)?;

                        self.handle_trades(trades, symbol, asset_id)?;
                        Ok(())
                    },
                    None => {
                        let internal_id = self.risk_engine.get_internal_id(user_id);
                        self.risk_engine.release(
                            internal_id,
                            order_id,
                            asset_id,
                            order.side,
                            order.price.unwrap_or(0),
                            order.initial_quantity,
                        );

                        let seq = self.next_seq(&symbol);
                        self.event_tx
                            .try_send(OrderBookEvent::OrderRejected(seq, symbol, order_id))
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

                let order_info = book.orders.get(&order_id).map(|entry| {
                    let o = entry.order.borrow();
                    (
                        o.user_id as ExternalUserId,
                        o.side,
                        o.price.unwrap_or(0),
                        o.remaining_quantity,
                    )
                });

                if book.cancel_order(order_id) {
                    if let Some((user_id, side, price, remaining_qty)) = order_info {
                        let internal_id = self.risk_engine.get_internal_id(user_id);
                        let asset_id = symbol.get_quantity_unit().asset_id();
                        self.risk_engine
                            .release(internal_id, order_id, asset_id, side, price, remaining_qty);
                    }

                    let seq = self.next_seq(&symbol);
                    self.event_tx
                        .try_send(OrderBookEvent::OrderCancelled(seq, symbol, order_id))
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
                        let seq = self.next_seq(&symbol);
                        self.event_tx
                            .try_send(OrderBookEvent::OrderModified(seq, symbol, modify_order))
                            .map_err(ExchangeError::ExchangeEventTrySendError)?;
                        let asset_id = symbol.get_quantity_unit().asset_id();
                        self.handle_trades(trades, symbol, asset_id)?;
                        Ok(())
                    },
                    None => {
                        let seq = self.next_seq(&symbol);
                        self.event_tx
                            .try_send(OrderBookEvent::ModifyOrderRejected(seq, symbol, modify_order))
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
                let seq = self.next_seq(&symbol);
                self.event_tx
                    .try_send(OrderBookEvent::OrdersExpired(seq, symbol, order_ids))
                    .map_err(ExchangeError::ExchangeEventTrySendError)?;
                Ok(())
            },
        }
    }
    pub fn handle_trades(&mut self, trades: Trades, symbol: Symbol, asset_id: AssetId) -> Result<(), ExchangeError> {
        for trade in trades {
            let ask_internal_id = self
                .risk_engine
                .get_internal_id(trade.ask_trade.user_id as ExternalUserId);
            let bid_internal_id = self
                .risk_engine
                .get_internal_id(trade.bid_trade.user_id as ExternalUserId);

            self.risk_engine.settle(
                bid_internal_id,
                trade.bid_trade.order_id,
                asset_id,
                Side::Buy,
                trade.bid_trade.price,
                trade.bid_trade.quantity,
            );

            self.risk_engine.settle(
                ask_internal_id,
                trade.ask_trade.order_id,
                asset_id,
                Side::Sell,
                trade.ask_trade.price,
                trade.ask_trade.quantity,
            );

            // ask_trade
            let (order_id, price, quantity) = {
                let order = trade.ask_trade;
                (order.order_id, order.price, order.quantity)
            };
            let seq = self.next_seq(&symbol);
            self.event_tx
                .try_send(OrderBookEvent::OrderMatched(seq, symbol, order_id, price, quantity))
                .map_err(ExchangeError::ExchangeEventTrySendError)?;
            // bid_trade
            let (order_id, price, quantity) = {
                let order = trade.bid_trade;
                (order.order_id, order.price, order.quantity)
            };
            let seq = self.next_seq(&symbol);
            self.event_tx
                .try_send(OrderBookEvent::OrderMatched(seq, symbol, order_id, price, quantity))
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

    type TestEventPoller = EventPoller<EventEnvelope, SingleProducerBarrier>;

    fn new_exchange() -> (Exchange, TestEventPoller) {
        let event_factory = || EventEnvelope { event: None };
        let builder = build_single_producer(1024, event_factory, BusySpin).with_multi_consumer();
        let (event_poller, builder) = builder.new_event_poller();
        let p = builder.build();
        let mut exchange = Exchange::new(p);
        let mut holdings = risk::risk_engine::Holdings::default();
        for symbol in Symbol::ALL {
            holdings.credit_asset(symbol.get_quantity_unit().asset_id(), 1_000_000);
        }
        exchange.get_risk_engine_mut().add_account(1, 1_000_000_000, holdings);
        (exchange, event_poller)
    }

    fn new_buy_fak(order_id: OrderId) -> NewOrder {
        NewOrder {
            order_id,
            user_id: 1,
            asset_id: 1,
            order_type: OrderType::FillAndKill,
            side: Side::Buy,
            price: Some(100),
            quantity: 1,
        }
    }
    fn new_sell_fak(order_id: OrderId) -> NewOrder {
        NewOrder {
            order_id,
            user_id: 1,
            asset_id: 1,
            order_type: OrderType::FillAndKill,
            side: Side::Sell,
            price: Some(100),
            quantity: 1,
        }
    }
    fn new_buy_gtc(order_id: OrderId) -> NewOrder {
        NewOrder {
            order_id,
            user_id: 1,
            asset_id: 1,
            order_type: OrderType::GoodTillCancel,
            side: Side::Buy,
            price: Some(100),
            quantity: 1,
        }
    }
    fn new_sell_gtc(order_id: OrderId) -> NewOrder {
        NewOrder {
            order_id,
            user_id: 1,
            asset_id: 1,
            order_type: OrderType::GoodTillCancel,
            side: Side::Sell,
            price: Some(100),
            quantity: 1,
        }
    }

    fn drain(poller: &mut TestEventPoller) -> Vec<OrderBookEvent> {
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
                OrderBookEvent::OrderAdded(1, SYMBOL, 1),
                OrderBookEvent::OrderAdded(2, SYMBOL, 2),
                OrderBookEvent::OrderMatched(3, SYMBOL, 2, 100, 1),
                OrderBookEvent::OrderMatched(4, SYMBOL, 1, 100, 1),
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

        assert_eq!(drain(&mut poller), vec![OrderBookEvent::OrderAdded(1, SYMBOL, 1)]);
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
        assert!(events.contains(&OrderBookEvent::OrderAdded(1, SYMBOL, 1)));
        assert!(events.contains(&OrderBookEvent::OrderMatched(4, SYMBOL, 1, 100, 1)));
        assert!(events.contains(&OrderBookEvent::OrderMatched(3, SYMBOL, 2, 100, 1)));
    }

    #[test]
    fn fak_order_rejected_when_nothing_to_cross() {
        const SYMBOL: Symbol = Symbol::BtcInr;
        let (mut exchange, mut poller) = new_exchange();
        exchange.add_new_orderbook(SYMBOL);

        exchange
            .handle_cmd(ExchangeCommand::AddNewOrder(SYMBOL, new_buy_fak(1)))
            .unwrap();

        assert_eq!(drain(&mut poller), vec![OrderBookEvent::OrderRejected(1, SYMBOL, 1)]);
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
        assert_eq!(drain(&mut poller), vec![OrderBookEvent::OrderCancelled(2, SYMBOL, 1)]);
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
                OrderBookEvent::OrderAdded(1, Symbol::BtcInr, 1),
                OrderBookEvent::OrderAdded(1, Symbol::EthInr, 2),
            ]
        );
        assert!(!events.iter().any(|e| matches!(e, OrderBookEvent::OrderMatched(..))));
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
            .filter(|e| matches!(e, OrderBookEvent::OrderMatched(..)))
            .count();

        assert_eq!(matched_count, Symbol::ALL.len() * 2);
    }

    #[test]
    fn test_risk_rejects_buy_order_when_insufficient_balance() {
        const SYMBOL: Symbol = Symbol::BtcInr;
        let event_factory = || EventEnvelope { event: None };
        let builder = build_single_producer(1024, event_factory, BusySpin).with_multi_consumer();
        let (mut poller, builder) = builder.new_event_poller();
        let p = builder.build();
        let mut exchange = Exchange::new(p);
        exchange.add_new_orderbook(SYMBOL);

        let order = NewOrder {
            order_id: 1,
            user_id: 99,
            asset_id: SYMBOL.get_quantity_unit().asset_id(),
            order_type: OrderType::GoodTillCancel,
            side: Side::Buy,
            price: Some(100),
            quantity: 10,
        };

        exchange
            .handle_cmd(ExchangeCommand::AddNewOrder(SYMBOL, order))
            .unwrap();

        assert_eq!(drain(&mut poller), vec![OrderBookEvent::OrderRejected(1, SYMBOL, 1)]);
        assert!(exchange.orderbooks[&SYMBOL].is_empty());
    }

    #[test]
    fn test_risk_settles_balances_on_trade_match() {
        const SYMBOL: Symbol = Symbol::BtcInr;
        let event_factory = || EventEnvelope { event: None };
        let builder = build_single_producer(1024, event_factory, BusySpin).with_multi_consumer();
        let (mut _poller, builder) = builder.new_event_poller();
        let p = builder.build();
        let mut exchange = Exchange::new(p);
        exchange.add_new_orderbook(SYMBOL);

        let asset_id = SYMBOL.get_quantity_unit().asset_id();
        let buyer_int = exchange
            .get_risk_engine_mut()
            .add_account(10, 1000, risk::risk_engine::Holdings::default());
        let seller_holdings = risk::risk_engine::Holdings::new(asset_id, 5);
        let seller_int = exchange.get_risk_engine_mut().add_account(20, 0, seller_holdings);

        let buy_order = NewOrder {
            order_id: 1,
            user_id: 10,
            asset_id,
            order_type: OrderType::GoodTillCancel,
            side: Side::Buy,
            price: Some(100),
            quantity: 2,
        };
        exchange
            .handle_cmd(ExchangeCommand::AddNewOrder(SYMBOL, buy_order))
            .unwrap();

        let sell_order = NewOrder {
            order_id: 2,
            user_id: 20,
            asset_id,
            order_type: OrderType::GoodTillCancel,
            side: Side::Sell,
            price: Some(100),
            quantity: 2,
        };
        exchange
            .handle_cmd(ExchangeCommand::AddNewOrder(SYMBOL, sell_order))
            .unwrap();

        let buyer_account = &exchange.get_risk_engine().accounts[buyer_int];
        assert_eq!(buyer_account.available_balance, 800);
        assert_eq!(buyer_account.reserved, 0);
        assert_eq!(buyer_account.holdings.get_available_quantity(asset_id), Some(2));

        let seller_account = &exchange.get_risk_engine().accounts[seller_int];
        assert_eq!(seller_account.available_balance, 200);
        assert_eq!(seller_account.reserved, 0);
        assert_eq!(seller_account.holdings.get_available_quantity(asset_id), Some(3));
    }

    #[test]
    fn test_risk_releases_on_order_cancelled() {
        const SYMBOL: Symbol = Symbol::BtcInr;
        let event_factory = || EventEnvelope { event: None };
        let builder = build_single_producer(1024, event_factory, BusySpin).with_multi_consumer();
        let (mut _poller, builder) = builder.new_event_poller();
        let p = builder.build();
        let mut exchange = Exchange::new(p);
        exchange.add_new_orderbook(SYMBOL);

        let asset_id = SYMBOL.get_quantity_unit().asset_id();
        let user_int = exchange
            .get_risk_engine_mut()
            .add_account(10, 1000, risk::risk_engine::Holdings::default());

        let buy_order = NewOrder {
            order_id: 1,
            user_id: 10,
            asset_id,
            order_type: OrderType::GoodTillCancel,
            side: Side::Buy,
            price: Some(100),
            quantity: 3,
        };
        exchange
            .handle_cmd(ExchangeCommand::AddNewOrder(SYMBOL, buy_order))
            .unwrap();

        assert_eq!(exchange.get_risk_engine().accounts[user_int].available_balance, 700);
        assert_eq!(exchange.get_risk_engine().accounts[user_int].reserved, 300);

        exchange.handle_cmd(ExchangeCommand::CancelOrder(SYMBOL, 1)).unwrap();

        assert_eq!(exchange.get_risk_engine().accounts[user_int].available_balance, 1000);
        assert_eq!(exchange.get_risk_engine().accounts[user_int].reserved, 0);
    }
}
