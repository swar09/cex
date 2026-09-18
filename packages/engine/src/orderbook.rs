use std::{
    cmp::{Reverse, min},
    collections::{BTreeMap, HashMap},
};

use chrono::{Local, Timelike};
use domain::{
    Quantity,
    level::PriceLevel,
    orders::{ModifyOrder, OrderIds, OrderPointer, OrderType},
    types::{OrderId, Price, Side, Trade, TradeInfo, Trades},
};

const START: u32 = 9;
const END: u32 = 16;

pub struct OrderEntry {
    pub order: OrderPointer,
    pub price: Price,
    pub side: Side,
    pub slab_key: usize,
}

pub type Orders = HashMap<OrderId, OrderEntry>;

pub struct OrderBook {
    pub asks: BTreeMap<Price, PriceLevel>,          // lowest price first
    pub bids: BTreeMap<Reverse<Price>, PriceLevel>, // highest price first
    pub orders: HashMap<OrderId, OrderEntry>,
    pub data: HashMap<Price, LevelData>,
}

pub struct LevelData {
    pub quantity: Quantity,
}
pub enum LevelDataAction {
    Add,
    Remove,
    Match,
}

impl OrderBook {
    pub fn new() -> Self {
        Self {
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
            orders: HashMap::new(),
            data: HashMap::new(),
        }
    }

    pub fn can_match(&self, side: Side, price: Price) -> bool {
        match side {
            Side::Buy => {
                // no one is selling
                if self.asks.is_empty() {
                    return false;
                }

                let best_ask = self.asks.first_key_value().unwrap();
                price >= *best_ask.0
            },
            Side::Sell => {
                // no one is buying
                if self.bids.is_empty() {
                    return false;
                }

                let (best_bid_price, _) = self.bids.first_key_value().unwrap();
                price <= best_bid_price.0
            },
        }
    }

    pub fn match_orders(&mut self) -> Trades {
        let mut trades: Trades = vec![];
        trades.reserve(self.orders.len());

        loop {
            if self.bids.is_empty() || self.asks.is_empty() {
                break;
            }

            let bid_price = self.bids.first_key_value().unwrap().0.0;
            let ask_price = *self.asks.first_key_value().unwrap().0;

            if bid_price < ask_price {
                break;
            }

            while !self.bids.is_empty() && !self.asks.is_empty() {
                let (bid_filled, bid_order_id, ask_filled, ask_order_id, quantity) = {
                    let (_, bid_level) = self.bids.first_key_value().unwrap();
                    let (_, ask_level) = self.asks.first_key_value().unwrap();

                    let mut bid = bid_level.front().unwrap().borrow_mut();
                    let mut ask = ask_level.front().unwrap().borrow_mut();

                    let quantity = min(bid.get_remaining_quantity(), ask.get_remaining_quantity());

                    bid.fill(quantity);
                    ask.fill(quantity);

                    (
                        bid.is_filled(),
                        bid.get_order_id(),
                        ask.is_filled(),
                        ask.get_order_id(),
                        quantity,
                    )
                };

                trades.push(Trade {
                    bid_trade: TradeInfo {
                        order_id: bid_order_id,
                        price: bid_price,
                        quantity,
                    },
                    ask_trade: TradeInfo {
                        order_id: ask_order_id,
                        price: ask_price,
                        quantity,
                    },
                });

                if bid_filled {
                    let level = self.bids.first_entry().unwrap().into_mut();
                    level.pop_front();
                    if level.is_empty() {
                        self.bids.pop_first();
                    }
                    self.orders.remove(&bid_order_id);
                }

                if ask_filled {
                    let level = self.asks.first_entry().unwrap().into_mut();
                    level.pop_front();
                    if level.is_empty() {
                        self.asks.pop_first();
                    }
                    self.orders.remove(&ask_order_id);
                }
            }
        }

        if !self.bids.is_empty() {
            let (_, bid_level) = self.bids.first_key_value().unwrap();
            let order = bid_level.front().unwrap().borrow();
            match order.get_order_type() {
                OrderType::FillAndKill => {
                    let order_id = order.get_order_id();
                    drop(order);
                    self.cancel_order(order_id);
                },
                OrderType::FillOrKill => {},
                OrderType::GoodTillCancel => {},
                OrderType::GoodForDay => {},
                OrderType::Market => {},
            }
        }

        if !self.asks.is_empty() {
            let (_, ask_level) = self.asks.first_key_value().unwrap();
            let order = ask_level.front().unwrap().borrow();
            match order.get_order_type() {
                OrderType::FillAndKill => {
                    let order_id = order.get_order_id();
                    drop(order);
                    self.cancel_order(order_id);
                },
                OrderType::FillOrKill => {},
                OrderType::GoodTillCancel => {},
                OrderType::GoodForDay => {},
                OrderType::Market => {},
            }
        }
        // self.on_order_matched(price, quantity);
        trades
    }

    pub fn add_order(&mut self, order: OrderPointer) -> Option<Trades> {
        let (order_type, order_id, order_side, order_price, _order_initial_quantity) = {
            (
                order.borrow().get_order_type(),
                order.borrow().get_order_id(),
                order.borrow().get_side(),
                order.borrow().get_price(),
                order.borrow().get_inital_quantity(),
            )
        };

        if order_type == OrderType::Market {
            if order_side == Side::Buy && !self.asks.is_empty() {
                let worst_ask_price = *self.asks.first_entry().unwrap().key();
                order.borrow_mut().to_good_till_cancel(worst_ask_price);
            } else if order_side == Side::Sell && !self.bids.is_empty() {
                let Reverse(worst_bid_price) = *self.bids.first_entry().unwrap().key();
                order.borrow_mut().to_good_till_cancel(worst_bid_price);
            } else {
                return None;
            }
        }

        if self.orders.contains_key(&order_id) {
            return None;
        }
        if order_type == OrderType::FillAndKill && !self.can_match(order_side, order_price) {
            return None;
        }

        let slab_key = match order_side {
            Side::Buy => self
                .bids
                .entry(Reverse(order_price))
                .or_insert_with(PriceLevel::new)
                .insert(order.clone()),
            Side::Sell => self
                .asks
                .entry(order_price)
                .or_insert_with(PriceLevel::new)
                .insert(order.clone()),
        };

        self.orders.insert(
            order_id,
            OrderEntry {
                order: order.clone(),
                price: order_price,
                side: order_side,
                slab_key,
            },
        );
        self.on_order_added(order_price, order.borrow().get_inital_quantity());
        Some(self.match_orders())
    }

    pub fn cancel_order(&mut self, order_id: OrderId) {
        // rename to cancel_order_internal
        let Some(order_entry) = self.orders.remove(&order_id) else {
            return;
        };

        match order_entry.side {
            Side::Buy => {
                let level = self.bids.get_mut(&Reverse(order_entry.price)).unwrap();
                level.remove(order_entry.slab_key);
                if level.is_empty() {
                    self.bids.remove(&Reverse(order_entry.price));
                }
                // self.on_order_cancelled(price, quantity);
            },
            Side::Sell => {
                let level = self.asks.get_mut(&order_entry.price).unwrap();
                level.remove(order_entry.slab_key);
                if level.is_empty() {
                    self.asks.remove(&order_entry.price);
                }
                // self.on_order_cancelled(price, quantity);
            },
        }

    }

    pub fn modify_order(&mut self, modify_order: ModifyOrder) -> Option<Trades> {
        if !self.orders.contains_key(&modify_order.order_id) {
            return None;
        }
        let existing_order_entry = self.orders.get(&modify_order.order_id).unwrap();
        let order_type = existing_order_entry.order.borrow().get_order_type();
        self.cancel_order(modify_order.order_id);
        self.add_order(modify_order.to_order_pointer(order_type))
    }

    pub fn len(&self) -> usize {
        self.orders.len()
    }
    pub fn cancel_orders(&mut self, order_ids: OrderIds) {
        for order_id in order_ids {
            self.cancel_order(order_id);
        }
    }
    pub fn can_fully_fill(&self, side: Side, price: Price, mut quantity: Quantity) -> bool {
        if !self.can_match(side, price) {
            return false;
        }

        let (threshold, price_level) = match side {
            Side::Buy => {
                let (ask_price, price_level) = self.asks.first_key_value().unwrap();
                (ask_price, price_level)
            },
            Side::Sell => {
                let (Reverse(bid_price), price_level) = self.bids.first_key_value().unwrap();
                (bid_price, price_level)
            },
        };

        for (level_price, level_data) in self.data.iter() {
            if (side == Side::Buy && threshold > level_price) || (side == Side::Sell && threshold < level_price) {
                continue;
            }

            if (side == Side::Buy && *level_price > price) || (side == Side::Sell && *level_price < price) {
                continue;
            }

            if quantity <= level_data.quantity {
                return true;
            }

            quantity -= level_data.quantity;
        }
        return false;
    }
    // events to maintain data
    pub fn on_order_cancelled(&mut self, price: Price, quantity: Quantity) {
        let Some(mut level_data) = self.data.get_mut(&price) else {
            return;
        };
        level_data.quantity -= quantity;
    }
    pub fn on_order_matched(&mut self, price: Price, quantity: Quantity) {
        let Some(mut level_data) = self.data.get_mut(&price) else {
            return;
        };
        level_data.quantity -= quantity;
    }
    pub fn on_order_added(&mut self, price: Price, quantity: Quantity) {
        let Some(mut level_data) = self.data.get_mut(&price) else {
            return;
        };
        level_data.quantity += quantity;
    }
    pub fn update_level_data(&mut self, price: Price, quantity: Quantity, action: LevelDataAction) {
        let Some(mut level_data) = self.data.get_mut(&price) else {
            return;
        };
        match action {
            LevelDataAction::Add => {
                self.on_order_added(price, quantity);
            },
            _ => {
                self.on_order_cancelled(price, quantity);
            },
        }
    }
    // pub fn shutdown() {}
}

impl Default for OrderBook {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc};

    use domain::{
        orders::{Order, OrderType},
        types::{OrderId, Price, Quantity, Side},
    };

    use super::*;

    fn make_order(id: OrderId, side: Side, price: Price, qty: Quantity, order_type: OrderType) -> OrderPointer {
        Rc::new(RefCell::new(Order {
            order_type,
            order_id: id,
            side,
            price: Some(price),
            intial_quantity: qty,
            remaining_quantity: qty,
        }))
    }

    fn gtc_buy(id: OrderId, price: Price, qty: Quantity) -> OrderPointer {
        make_order(id, Side::Buy, price, qty, OrderType::GoodTillCancel)
    }

    fn gtc_sell(id: OrderId, price: Price, qty: Quantity) -> OrderPointer {
        make_order(id, Side::Sell, price, qty, OrderType::GoodTillCancel)
    }

    fn fak_buy(id: OrderId, price: Price, qty: Quantity) -> OrderPointer {
        make_order(id, Side::Buy, price, qty, OrderType::FillAndKill)
    }

    fn fak_sell(id: OrderId, price: Price, qty: Quantity) -> OrderPointer {
        make_order(id, Side::Sell, price, qty, OrderType::FillAndKill)
    }

    #[test]
    fn test_add_single_buy_order() {
        let mut book = OrderBook::new();
        let order = gtc_buy(1, 100, 10);
        let trades = book.add_order(order);

        assert!(trades.is_some());
        assert!(trades.unwrap().is_empty());
        assert!(book.orders.contains_key(&1)); // order stored
        assert!(book.bids.contains_key(&Reverse(100))); // price level exists
    }

    #[test]
    fn test_add_single_sell_order() {
        let mut book = OrderBook::new();
        let order = gtc_sell(1, 100, 10);
        let trades = book.add_order(order);

        assert!(trades.is_some());
        assert!(trades.unwrap().is_empty());
        assert!(book.orders.contains_key(&1));
        assert!(book.asks.contains_key(&100));
    }

    #[test]
    fn test_duplicate_order_id_rejected() {
        let mut book = OrderBook::new();
        book.add_order(gtc_buy(1, 100, 10));

        // same id again
        let result = book.add_order(gtc_buy(1, 200, 5));
        assert!(result.is_none()); // rejected
        assert_eq!(book.orders.len(), 1); // still only 1 order
    }

    #[test]
    fn test_multiple_orders_same_price_level() {
        let mut book = OrderBook::new();
        book.add_order(gtc_buy(1, 100, 10));
        book.add_order(gtc_buy(2, 100, 20));
        book.add_order(gtc_buy(3, 100, 30));

        assert_eq!(book.orders.len(), 3);
        // all at same price level
        let level = book.bids.get(&Reverse(100)).unwrap();
        assert_eq!(level.orders.len(), 3);
    }

    #[test]
    fn test_cancel_buy_order() {
        let mut book = OrderBook::new();
        book.add_order(gtc_buy(1, 100, 10));

        book.cancel_order(1);

        assert!(!book.orders.contains_key(&1)); // removed from orders
        assert!(!book.bids.contains_key(&Reverse(100))); // price level cleaned up
    }

    #[test]
    fn test_cancel_sell_order() {
        let mut book = OrderBook::new();
        book.add_order(gtc_sell(1, 100, 10));

        book.cancel_order(1);

        assert!(!book.orders.contains_key(&1));
        assert!(!book.asks.contains_key(&100));
    }

    #[test]
    fn test_cancel_nonexistent_order() {
        let mut book = OrderBook::new();
        // should not panic
        book.cancel_order(999);
    }

    #[test]
    fn test_cancel_one_order_price_level_remains() {
        let mut book = OrderBook::new();
        book.add_order(gtc_buy(1, 100, 10));
        book.add_order(gtc_buy(2, 100, 20)); // same price level

        book.cancel_order(1); // cancel only one

        // price level still exists for order 2
        assert!(book.bids.contains_key(&Reverse(100)));
        assert!(book.orders.contains_key(&2));
        assert!(!book.orders.contains_key(&1));

        let level = book.bids.get(&Reverse(100)).unwrap();
        assert_eq!(level.orders.len(), 1); // only order 2 remains
    }

    #[test]
    fn test_cancel_all_orders_price_level_removed() {
        let mut book = OrderBook::new();
        book.add_order(gtc_buy(1, 100, 10));
        book.add_order(gtc_buy(2, 100, 20));

        book.cancel_order(1);
        book.cancel_order(2);

        // price level should be gone
        assert!(!book.bids.contains_key(&Reverse(100)));
        assert!(book.orders.is_empty());
    }

    #[test]
    fn test_full_match_single_order() {
        let mut book = OrderBook::new();
        book.add_order(gtc_buy(1, 100, 10));
        let trades = book.add_order(gtc_sell(2, 100, 10)).unwrap();

        assert_eq!(trades.len(), 1);
        assert_eq!(trades[0].bid_trade.order_id, 1);
        assert_eq!(trades[0].ask_trade.order_id, 2);
        assert_eq!(trades[0].bid_trade.quantity, 10);
        assert_eq!(trades[0].ask_trade.quantity, 10);

        // both fully filled, both removed
        assert!(book.orders.is_empty());
        assert!(book.bids.is_empty());
        assert!(book.asks.is_empty());
    }

    #[test]
    fn test_partial_match_bid_remains() {
        let mut book = OrderBook::new();
        book.add_order(gtc_buy(1, 100, 20)); // buy 20
        let trades = book.add_order(gtc_sell(2, 100, 10)).unwrap(); // sell 10

        // bid still alive with 10 remaining
        assert!(book.orders.contains_key(&1));
        assert!(!book.orders.contains_key(&2)); // ask fully filled
        assert_eq!(book.orders[&1].order.borrow().get_remaining_quantity(), 10);
    }

    #[test]
    fn test_partial_match_ask_remains() {
        let mut book = OrderBook::new();
        book.add_order(gtc_buy(1, 100, 10)); // buy 10
        let trades = book.add_order(gtc_sell(2, 100, 20)).unwrap(); // sell 20

        assert!(!book.orders.contains_key(&1)); // bid fully filled
        assert!(book.orders.contains_key(&2)); // ask remains
        assert_eq!(book.orders[&2].order.borrow().get_remaining_quantity(), 10);
    }

    #[test]
    fn test_no_match_price_mismatch() {
        let mut book = OrderBook::new();
        book.add_order(gtc_buy(1, 90, 10)); // buy at 90
        let trades = book.add_order(gtc_sell(2, 100, 10)).unwrap(); // sell at 100

        // 90 < 100, no match
        assert!(trades.is_empty());
        assert_eq!(book.orders.len(), 2); // both still alive
    }

    #[test]
    fn test_fifo_matching_order() {
        let mut book = OrderBook::new();

        // two buys at same price, order 1 should match first (FIFO)
        book.add_order(gtc_buy(1, 100, 10));
        book.add_order(gtc_buy(2, 100, 10));

        let trades = book.add_order(gtc_sell(3, 100, 10)).unwrap();

        assert_eq!(trades.len(), 1);
        assert_eq!(trades[0].bid_trade.order_id, 1); // order 1 matched first
        assert!(book.orders.contains_key(&2)); // order 2 still alive
    }

    #[test]
    fn test_price_priority_bids() {
        let mut book = OrderBook::new();

        // higher bid should match first
        book.add_order(gtc_buy(1, 90, 10));
        book.add_order(gtc_buy(2, 100, 10)); // better price

        let trades = book.add_order(gtc_sell(3, 90, 10)).unwrap();
        assert_eq!(trades[0].bid_trade.order_id, 2); // highest bid matched
        assert_eq!(trades[0].bid_trade.price, 100); // highest bid price
        assert!(book.orders.contains_key(&1)); // lower bid still alive
    }

    #[test]
    fn test_multiple_trades_one_sell() {
        let mut book = OrderBook::new();
        book.add_order(gtc_buy(1, 100, 5));
        book.add_order(gtc_buy(2, 100, 5));
        book.add_order(gtc_buy(3, 100, 5));

        // one sell matches all three buys
        let trades = book.add_order(gtc_sell(4, 100, 15)).unwrap();

        assert_eq!(trades.len(), 3);
        assert_eq!(trades[0].bid_trade.order_id, 1);
        assert_eq!(trades[1].bid_trade.order_id, 2);
        assert_eq!(trades[2].bid_trade.order_id, 3);
        assert!(book.orders.is_empty());
    }

    #[test]
    fn test_fak_no_match_rejected() {
        let mut book = OrderBook::new();

        // no asks exist, FAK buy should be rejected
        let result = book.add_order(fak_buy(1, 100, 10));
        assert!(result.is_none());
        assert!(book.orders.is_empty());
    }

    #[test]
    fn test_fak_full_match() {
        let mut book = OrderBook::new();
        book.add_order(gtc_sell(1, 100, 10));

        let trades = book.add_order(fak_buy(2, 100, 10)).unwrap();

        assert_eq!(trades.len(), 1);
        assert!(book.orders.is_empty());
    }

    #[test]
    fn test_fak_partial_match_remainder_cancelled() {
        let mut book = OrderBook::new();
        book.add_order(gtc_sell(1, 100, 5));

        // FAK buy 10 but only 5 available - remaining 5 cancelled
        let trades = book.add_order(fak_buy(2, 100, 10)).unwrap();

        assert_eq!(trades.len(), 1);
        assert_eq!(trades[0].bid_trade.quantity, 5);
        assert!(!book.orders.contains_key(&2)); // FAK remainder cancelled
    }

    #[test]
    fn test_slab_key_stable_after_other_cancel() {
        let mut book = OrderBook::new();
        book.add_order(gtc_buy(1, 100, 10));
        book.add_order(gtc_buy(2, 100, 20));
        book.add_order(gtc_buy(3, 100, 30));

        // cancel middle order
        book.cancel_order(2);

        // remaining orders still accessible via their slab keys
        assert!(book.orders.contains_key(&1));
        assert!(book.orders.contains_key(&3));

        let level = book.bids.get(&Reverse(100)).unwrap();
        let key1 = book.orders[&1].slab_key;
        let key3 = book.orders[&3].slab_key;

        // slab keys still valid
        assert_eq!(level.orders[key1].borrow().get_order_id(), 1);
        assert_eq!(level.orders[key3].borrow().get_order_id(), 3);
    }

    #[test]
    fn test_cancel_then_add_reuses_slab_slot() {
        let mut book = OrderBook::new();
        book.add_order(gtc_buy(1, 100, 10));
        book.cancel_order(1); // slot freed

        book.add_order(gtc_buy(2, 100, 20)); // should reuse slot

        let level = book.bids.get(&Reverse(100)).unwrap();
        assert_eq!(level.orders.len(), 1);
        assert!(book.orders.contains_key(&2));
    }

    #[test]
    fn test_modify_order() {
        let mut book = OrderBook::new();
        book.add_order(gtc_buy(1, 100, 10));
        let modify_order = ModifyOrder {
            order_id: 1,
            order_type: OrderType::GoodTillCancel,
            side: Side::Sell,
            price: 200,
            quantity: 20,
        };
        book.add_order(gtc_buy(2, 200, 20));
        let trades = book.modify_order(modify_order).unwrap();
        assert_eq!(trades.len(), 1);
        assert_eq!(trades[0].bid_trade.order_id, 2);
        assert_eq!(trades[0].bid_trade.price, 200);
        assert_eq!(trades[0].bid_trade.quantity, 20);
    }

    #[test]
    fn test_orderbook_len() {
        let mut book = OrderBook::new();
        for i in 1..=10 {
            book.add_order(gtc_buy(i, 100, 10));
        }
        assert_eq!(book.len(), 10);
    }
}
