use std::{
    cmp::{Reverse, min},
    collections::BTreeMap,
};

use domain::{
    NewOrder, Order, Quantity, UserId,
    orders::{ModifyOrder, OrderIds, OrderType},
    types::{OrderId, Price, Side, Trade, TradeInfo, Trades},
};
use fxhash::FxHashMap;
use slab::Slab;

#[derive(Clone, Copy, Debug)]
pub struct PriceLevelNode {
    pub order: Order,
    pub prev: usize,
    pub next: usize,
}

impl std::ops::Deref for PriceLevelNode {
    type Target = Order;
    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.order
    }
}

impl std::ops::DerefMut for PriceLevelNode {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.order
    }
}

#[derive(Clone, Debug)]
pub struct PriceLevel {
    pub price: Price,
    pub orders: Slab<PriceLevelNode>,
    pub head: usize,
    pub tail: usize,
    pub total_quantity: Quantity,
}

impl PriceLevel {
    pub fn new() -> Self {
        Self {
            price: 0,
            orders: Slab::new(),
            head: usize::MAX,
            tail: usize::MAX,
            total_quantity: 0,
        }
    }

    pub fn with_price(price: Price) -> Self {
        Self {
            price,
            orders: Slab::new(),
            head: usize::MAX,
            tail: usize::MAX,
            total_quantity: 0,
        }
    }

    #[inline(always)]
    pub fn insert(&mut self, order: Order) -> usize {
        self.total_quantity += order.remaining_quantity;
        let key = self.orders.insert(PriceLevelNode {
            order,
            prev: self.tail,
            next: usize::MAX,
        });
        if self.tail != usize::MAX {
            self.orders[self.tail].next = key;
        } else {
            self.head = key;
        }
        self.tail = key;
        key
    }

    #[inline(always)]
    pub fn remove(&mut self, key: usize) -> Order {
        let node = self.orders.remove(key);
        self.total_quantity -= node.order.remaining_quantity;
        if node.prev != usize::MAX {
            self.orders[node.prev].next = node.next;
        } else {
            self.head = node.next;
        }
        if node.next != usize::MAX {
            self.orders[node.next].prev = node.prev;
        } else {
            self.tail = node.prev;
        }
        node.order
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.orders.is_empty()
    }

    #[inline]
    pub fn front(&self) -> Option<&Order> {
        if self.head == usize::MAX {
            None
        } else {
            Some(&self.orders[self.head].order)
        }
    }

    #[inline]
    pub fn front_mut(&mut self) -> Option<&mut Order> {
        if self.head == usize::MAX {
            None
        } else {
            Some(&mut self.orders[self.head].order)
        }
    }

    #[inline]
    pub fn pop_front(&mut self) -> Option<Order> {
        if self.head == usize::MAX { None } else { Some(self.remove(self.head)) }
    }

    pub fn iter(&self) -> impl Iterator<Item = &Order> {
        self.orders.iter().map(|(_, n)| &n.order)
    }
}

impl Default for PriceLevel {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OrderEntry {
    pub order_id: OrderId,
    pub user_id: UserId,
    pub price: Price,
    pub side: Side,
    pub order_type: OrderType,
    pub slab_key: usize,
}

pub type Orders = FxHashMap<OrderId, OrderEntry>;

pub struct OrderBook {
    pub asks: BTreeMap<Price, PriceLevel>,          // lowest price first
    pub bids: BTreeMap<Reverse<Price>, PriceLevel>, // highest price first
    pub orders: FxHashMap<OrderId, OrderEntry>,
    pub last_trade_price: Option<Price>,
}

impl OrderBook {
    pub fn new() -> Self {
        Self {
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
            orders: FxHashMap::default(),
            last_trade_price: None,
        }
    }

    #[inline]
    pub fn get_order_entry(&self, order_id: OrderId) -> Option<&OrderEntry> {
        self.orders.get(&order_id)
    }

    #[inline]
    pub fn get_order(&self, order_id: OrderId) -> Option<&Order> {
        let entry = self.orders.get(&order_id)?;
        match entry.side {
            Side::Buy => self
                .bids
                .get(&Reverse(entry.price))
                .and_then(|l| l.orders.get(entry.slab_key))
                .map(|n| &n.order),
            Side::Sell => self.asks.get(&entry.price).and_then(|l| l.orders.get(entry.slab_key)).map(|n| &n.order),
        }
    }

    pub fn iter_orders(&self) -> impl Iterator<Item = &Order> {
        self.asks.values().flat_map(|l| l.iter()).chain(self.bids.values().flat_map(|l| l.iter()))
    }

    #[inline]
    pub fn get_market_price(&self, side: Side) -> Option<Price> {
        match side {
            Side::Buy => self.asks.first_key_value().map(|(p, _)| *p).or(self.last_trade_price),
            Side::Sell => self.bids.first_key_value().map(|(Reverse(p), _)| *p).or(self.last_trade_price),
        }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.orders.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.orders.is_empty()
    }

    #[inline(always)]
    pub fn can_match(&self, side: Side, price: Price) -> bool {
        match side {
            Side::Buy => {
                if self.asks.is_empty() {
                    return false;
                }
                let best_ask = self.asks.first_key_value().expect("asks empty checked above");
                price >= *best_ask.0
            },
            Side::Sell => {
                if self.bids.is_empty() {
                    return false;
                }
                let (best_bid_price, _) = self.bids.first_key_value().expect("bids empty check above");
                price <= best_bid_price.0
            },
        }
    }

    pub fn match_orders(&mut self) -> Trades {
        let mut trades: Trades = Trades::new();

        loop {
            if self.bids.is_empty() || self.asks.is_empty() {
                break;
            }

            let bid_price = self.bids.first_key_value().expect("bids empty check above").0.0;
            let ask_price = *self.asks.first_key_value().expect("asks empty check above").0;

            if bid_price < ask_price {
                break;
            }

            while !self.bids.is_empty() && !self.asks.is_empty() {
                let (bid_filled, bid_order_id, bid_user_id, bid_rem, ask_filled, ask_order_id, ask_user_id, ask_rem, quantity, self_trade) = {
                    let bid_level = self.bids.values_mut().next().expect("bids empty check above");
                    let ask_level = self.asks.values_mut().next().expect("asks empty check above");

                    let (bid_filled, bid_order_id, bid_user_id, bid_rem, ask_filled, ask_order_id, ask_user_id, ask_rem, quantity, self_trade) = {
                        let bid = bid_level.front_mut().expect("cannot get bid order");
                        let ask = ask_level.front_mut().expect("cannot get ask order");

                        if bid.get_user_id() == ask.get_user_id() {
                            (
                                false,
                                bid.get_order_id(),
                                bid.get_user_id(),
                                0,
                                false,
                                ask.get_order_id(),
                                ask.get_user_id(),
                                0,
                                0,
                                true,
                            )
                        } else {
                            let quantity = min(bid.get_remaining_quantity(), ask.get_remaining_quantity());

                            bid.fill(quantity);
                            ask.fill(quantity);

                            (
                                bid.is_filled(),
                                bid.get_order_id(),
                                bid.get_user_id(),
                                bid.get_remaining_quantity(),
                                ask.is_filled(),
                                ask.get_order_id(),
                                ask.get_user_id(),
                                ask.get_remaining_quantity(),
                                quantity,
                                false,
                            )
                        }
                    };

                    if !self_trade {
                        bid_level.total_quantity -= quantity;
                        ask_level.total_quantity -= quantity;
                    }

                    (
                        bid_filled,
                        bid_order_id,
                        bid_user_id,
                        bid_rem,
                        ask_filled,
                        ask_order_id,
                        ask_user_id,
                        ask_rem,
                        quantity,
                        self_trade,
                    )
                };

                if self_trade {
                    self.cancel_order(bid_order_id);
                    self.cancel_order(ask_order_id);
                    break;
                }

                self.last_trade_price = Some(ask_price);

                trades.push(Trade {
                    bid_trade: TradeInfo {
                        order_id: bid_order_id,
                        user_id: bid_user_id,
                        price: bid_price,
                        quantity,
                        remaining_quantity: bid_rem,
                    },
                    ask_trade: TradeInfo {
                        order_id: ask_order_id,
                        user_id: ask_user_id,
                        price: ask_price,
                        quantity,
                        remaining_quantity: ask_rem,
                    },
                });

                let mut level_depleted = false;

                if bid_filled {
                    let level = self.bids.first_entry().expect("bids empty check above").into_mut();
                    level.pop_front();
                    if level.is_empty() {
                        self.bids.pop_first();
                        level_depleted = true;
                    }
                    self.orders.remove(&bid_order_id);
                }

                if ask_filled {
                    let level = self.asks.first_entry().expect("asks empty check above").into_mut();
                    level.pop_front();
                    if level.is_empty() {
                        self.asks.pop_first();
                        level_depleted = true;
                    }
                    self.orders.remove(&ask_order_id);
                }

                if level_depleted {
                    break;
                }
            }
        }

        if let Some(bid_level) = self.bids.values().next()
            && let Some(order) = bid_level.front()
            && (order.get_order_type() == OrderType::FillAndKill || order.get_order_type() == OrderType::FillOrKill)
        {
            let order_id = order.get_order_id();
            self.cancel_order(order_id);
        }

        if let Some(ask_level) = self.asks.values().next()
            && let Some(order) = ask_level.front()
            && (order.get_order_type() == OrderType::FillAndKill || order.get_order_type() == OrderType::FillOrKill)
        {
            let order_id = order.get_order_id();
            self.cancel_order(order_id);
        }
        trades
    }

    #[inline]
    pub fn add_new_order(&mut self, order: NewOrder) -> Option<Trades> {
        self.add_order(Order::from(order))
    }

    pub fn add_order(&mut self, mut order: Order) -> Option<Trades> {
        let mut order_type = order.get_order_type();
        let order_id = order.get_order_id();
        let order_user_id = order.get_user_id();
        let order_side = order.get_side();

        if order_type == OrderType::Market {
            if order_side == Side::Buy && !self.asks.is_empty() {
                let worst_ask_price = *self.asks.first_entry().expect("asks not empty check above").key();
                order.to_fill_or_kill(worst_ask_price);
            } else if order_side == Side::Sell && !self.bids.is_empty() {
                let Reverse(worst_bid_price) = *self.bids.first_entry().expect("bids not empty check above").key();
                order.to_fill_or_kill(worst_bid_price);
            } else {
                let last_price = self.last_trade_price?;
                order.to_fill_or_kill(last_price);
            }
            order_type = OrderType::FillOrKill;
        }

        let order_price = order.get_price();

        let can_match = self.can_match(order_side, order_price);
        if order_type == OrderType::FillAndKill && !can_match {
            return None;
        }
        if order_type == OrderType::FillOrKill && !self.can_fully_fill(order_side, order_price, order.get_remaining_quantity()) {
            return None;
        }

        let orders_entry = match self.orders.entry(order_id) {
            std::collections::hash_map::Entry::Occupied(_) => return None,
            std::collections::hash_map::Entry::Vacant(v) => v,
        };

        let slab_key = match order_side {
            Side::Buy => self.bids.entry(Reverse(order_price)).or_default().insert(order),
            Side::Sell => self.asks.entry(order_price).or_default().insert(order),
        };

        orders_entry.insert(OrderEntry {
            order_id,
            user_id: order_user_id,
            price: order_price,
            side: order_side,
            order_type,
            slab_key,
        });
        if can_match { Some(self.match_orders()) } else { Some(Trades::new()) }
    }

    pub fn cancel_order(&mut self, order_id: OrderId) -> Option<Order> {
        let order_entry = self.orders.remove(&order_id)?;

        let order = match order_entry.side {
            Side::Buy => {
                let level = self
                    .bids
                    .get_mut(&Reverse(order_entry.price))
                    .expect("Order & PriceLevel exists check above");
                let removed = level.remove(order_entry.slab_key);
                if level.is_empty() {
                    self.bids.remove(&Reverse(order_entry.price));
                }
                removed
            },
            Side::Sell => {
                let level = self.asks.get_mut(&order_entry.price).expect("Order & PriceLevel exists check above");
                let removed = level.remove(order_entry.slab_key);
                if level.is_empty() {
                    self.asks.remove(&order_entry.price);
                }
                removed
            },
        };
        Some(order)
    }

    #[inline]
    pub fn modify_order(&mut self, modify_order: ModifyOrder) -> Option<(Order, Trades)> {
        let old_order = self.cancel_order(modify_order.order_id)?;
        let order = Order {
            order_id: modify_order.get_order_id(),
            user_id: old_order.user_id,
            asset_id: old_order.asset_id,
            price: Some(modify_order.get_price()),
            initial_quantity: modify_order.get_quantity(),
            remaining_quantity: modify_order.get_quantity(),
            order_type: old_order.order_type,
            side: modify_order.get_side(),
        };
        let trades = self.add_order(order)?;
        Some((old_order, trades))
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

        match side {
            Side::Buy => {
                for (&ask_price, level) in self.asks.iter() {
                    if ask_price > price {
                        break;
                    }
                    if quantity <= level.total_quantity {
                        return true;
                    }
                    quantity -= level.total_quantity;
                }
            },
            Side::Sell => {
                for (&Reverse(bid_price), level) in self.bids.iter() {
                    if bid_price < price {
                        break;
                    }
                    if quantity <= level.total_quantity {
                        return true;
                    }
                    quantity -= level.total_quantity;
                }
            },
        }
        false
    }
}

impl Default for OrderBook {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use domain::{
        orders::{Order, OrderType},
        types::{OrderId, Price, Quantity, Side},
    };

    use super::*;

    fn make_order(id: OrderId, side: Side, price: Price, qty: Quantity, order_type: OrderType) -> Order {
        Order {
            order_id: id,
            user_id: id,
            asset_id: 1,
            price: Some(price),
            initial_quantity: qty,
            remaining_quantity: qty,
            order_type,
            side,
        }
    }

    fn gtc_buy(id: OrderId, price: Price, qty: Quantity) -> Order {
        make_order(id, Side::Buy, price, qty, OrderType::GoodTillCancel)
    }

    fn gtc_sell(id: OrderId, price: Price, qty: Quantity) -> Order {
        make_order(id, Side::Sell, price, qty, OrderType::GoodTillCancel)
    }

    fn fak_buy(id: OrderId, price: Price, qty: Quantity) -> Order {
        make_order(id, Side::Buy, price, qty, OrderType::FillAndKill)
    }

    fn fak_sell(id: OrderId, price: Price, qty: Quantity) -> Order {
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
        let _trades = book.add_order(gtc_sell(2, 100, 10)).unwrap(); // sell 10

        // bid still alive with 10 remaining
        assert!(book.orders.contains_key(&1));
        assert!(!book.orders.contains_key(&2)); // ask fully filled
        assert_eq!(book.get_order(1).unwrap().get_remaining_quantity(), 10);
    }

    #[test]
    fn test_partial_match_ask_remains() {
        let mut book = OrderBook::new();
        book.add_order(gtc_buy(1, 100, 10)); // buy 10
        let _trades = book.add_order(gtc_sell(2, 100, 20)).unwrap(); // sell 20

        assert!(!book.orders.contains_key(&1)); // bid fully filled
        assert!(book.orders.contains_key(&2)); // ask remains
        assert_eq!(book.get_order(2).unwrap().get_remaining_quantity(), 10);
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

        // no asks exist, FAK sell should be rejected
        let result = book.add_order(fak_sell(1, 100, 10));
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
        assert_eq!(book.orders.len(), 0);
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
        assert_eq!(level.orders[key1].get_order_id(), 1);
        assert_eq!(level.orders[key3].get_order_id(), 3);
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
        let (_old_order, trades) = book.modify_order(modify_order).unwrap();
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
    #[test]
    fn test_can_fully_fill_true_when_enough_resting_liquidity() {
        let mut book = OrderBook::new();
        book.add_order(gtc_sell(1, 100, 10));
        book.add_order(gtc_sell(2, 100, 5)); // total resting at 100 = 15

        // a buy of 15 at price 100 should be fully fillable
        assert!(book.can_fully_fill(Side::Buy, 100, 15));
    }

    #[test]
    fn test_can_fully_fill_false_when_not_enough_liquidity() {
        let mut book = OrderBook::new();
        book.add_order(gtc_sell(1, 100, 5));

        // asking to fill more than what's resting should fail
        assert!(!book.can_fully_fill(Side::Buy, 100, 10));
    }

    #[test]
    fn test_can_fully_fill_false_after_liquidity_consumed_by_match() {
        let mut book = OrderBook::new();
        book.add_order(gtc_sell(1, 100, 10));

        // consume all resting liquidity via a match
        book.add_order(gtc_buy(2, 100, 10));

        // nothing left resting at 100, so a fresh buy shouldn't be fully fillable
        assert!(!book.can_fully_fill(Side::Buy, 100, 1));
    }

    #[test]
    fn test_can_fully_fill_reflects_partial_fill_remainder() {
        let mut book = OrderBook::new();
        book.add_order(gtc_sell(1, 100, 10));

        // partially match away 4, leaving 6 resting
        book.add_order(gtc_buy(2, 100, 4));

        assert!(book.can_fully_fill(Side::Buy, 100, 6)); // exactly what remains
        // assert!(book.can_fully_fill(Side::Buy, 100, 7)); // more than what
        // remains
    }

    #[test]
    fn test_can_fully_fill_false_after_cancel_removes_liquidity() {
        let mut book = OrderBook::new();
        book.add_order(gtc_sell(1, 100, 10));
        book.add_order(gtc_sell(2, 100, 10)); // 20 resting total

        book.cancel_order(1); // cancel half the liquidity

        assert!(book.can_fully_fill(Side::Buy, 100, 10)); // remaining order covers it
        assert!(!book.can_fully_fill(Side::Buy, 100, 15)); // cancelled qty no longer counted
    }

    #[test]
    fn test_can_fully_fill_false_after_all_orders_cancelled() {
        let mut book = OrderBook::new();
        book.add_order(gtc_sell(1, 100, 10));
        book.cancel_order(1);

        // level is gone entirely, nothing to fill against
        assert!(!book.can_fully_fill(Side::Buy, 100, 1));
    }

    #[test]
    fn test_can_fully_fill_accounts_for_multiple_price_levels_walked_through() {
        let mut book = OrderBook::new();
        book.add_order(gtc_sell(1, 100, 5));
        book.add_order(gtc_sell(2, 101, 5));

        // buying 10 with a limit of 101 should walk through both levels
        assert!(book.can_fully_fill(Side::Buy, 101, 10));
        // but asking for more than total available across eligible levels should fail
        assert!(!book.can_fully_fill(Side::Buy, 101, 11));
    }

    #[test]
    fn test_can_fully_fill_sell_side_uses_bid_liquidity() {
        let mut book = OrderBook::new();
        book.add_order(gtc_buy(1, 100, 10));
        book.add_order(gtc_buy(2, 100, 5)); // 15 resting bid liquidity

        assert!(book.can_fully_fill(Side::Sell, 100, 15));
        assert!(!book.can_fully_fill(Side::Sell, 100, 16));
    }

    #[test]
    fn test_self_trade_prevention_cancels_both_orders() {
        let mut book = OrderBook::new();
        let user_id = 42;
        let buy = Order {
            order_id: 1,
            user_id,
            asset_id: 1,
            price: Some(100),
            initial_quantity: 10,
            remaining_quantity: 10,
            order_type: OrderType::GoodTillCancel,
            side: Side::Buy,
        };
        let sell = Order {
            order_id: 2,
            user_id,
            asset_id: 1,
            price: Some(100),
            initial_quantity: 10,
            remaining_quantity: 10,
            order_type: OrderType::GoodTillCancel,
            side: Side::Sell,
        };

        book.add_order(buy);
        assert_eq!(book.len(), 1);

        let trades = book.add_order(sell).unwrap();
        assert!(trades.is_empty());
        assert_eq!(book.len(), 0);
        assert!(!book.orders.contains_key(&1));
        assert!(!book.orders.contains_key(&2));
        assert!(book.bids.is_empty());
        assert!(book.asks.is_empty());
    }

    #[test]
    fn test_market_buy_fully_fills_when_liquidity_sufficient() {
        let mut book = OrderBook::new();
        book.add_order(gtc_sell(1, 100, 10));

        // Market buy of 10 against resting ask of 10 -> fully fills
        let market_buy = Order::new_market_order(2, 2, 1, Side::Buy, 10, OrderType::Market);
        let trades = book.add_order(market_buy).unwrap();

        assert_eq!(trades.len(), 1);
        assert_eq!(trades[0].bid_trade.quantity, 10);
        assert_eq!(trades[0].bid_trade.price, 100);

        // Entire order filled, nothing rests in the book
        assert_eq!(book.len(), 0);
        assert!(!book.orders.contains_key(&2));
        assert!(book.bids.is_empty());
        assert!(book.asks.is_empty());
    }

    #[test]
    fn test_market_buy_killed_when_insufficient_liquidity() {
        let mut book = OrderBook::new();
        book.add_order(gtc_sell(1, 100, 5));

        // Market buy of 10 against resting ask of only 5 -> Fill Or Kill kills the
        // order
        let market_buy = Order::new_market_order(2, 2, 1, Side::Buy, 10, OrderType::Market);
        assert!(book.add_order(market_buy).is_none());

        // Resting ask remains completely untouched!
        assert_eq!(book.len(), 1);
        assert!(book.orders.contains_key(&1));
        assert_eq!(book.asks[&100].total_quantity, 5);
    }

    #[test]
    fn test_market_sell_fully_fills_when_liquidity_sufficient() {
        let mut book = OrderBook::new();
        book.add_order(gtc_buy(1, 100, 10));

        // Market sell of 10 against resting bid of 10 -> fully fills
        let market_sell = Order::new_market_order(2, 2, 1, Side::Sell, 10, OrderType::Market);
        let trades = book.add_order(market_sell).unwrap();

        assert_eq!(trades.len(), 1);
        assert_eq!(trades[0].ask_trade.quantity, 10);
        assert_eq!(trades[0].ask_trade.price, 100);

        // Entire order filled, nothing rests in the book
        assert_eq!(book.len(), 0);
        assert!(!book.orders.contains_key(&2));
        assert!(book.bids.is_empty());
        assert!(book.asks.is_empty());
    }

    #[test]
    fn test_market_sell_killed_when_insufficient_liquidity() {
        let mut book = OrderBook::new();
        book.add_order(gtc_buy(1, 100, 5));

        // Market sell of 10 against resting bid of only 5 -> Fill Or Kill kills the
        // order
        let market_sell = Order::new_market_order(2, 2, 1, Side::Sell, 10, OrderType::Market);
        assert!(book.add_order(market_sell).is_none());

        // Resting bid remains completely untouched!
        assert_eq!(book.len(), 1);
        assert!(book.orders.contains_key(&1));
        assert_eq!(book.bids[&Reverse(100)].total_quantity, 5);
    }

    #[test]
    fn test_market_order_rejected_when_no_liquidity() {
        let mut book = OrderBook::new();
        let market_buy = Order::new_market_order(1, 1, 1, Side::Buy, 10, OrderType::Market);
        assert!(book.add_order(market_buy).is_none());
    }
}
