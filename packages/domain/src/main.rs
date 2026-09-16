use std::{
    cell::RefCell,
    cmp::{Reverse, min},
    collections::{BTreeMap, HashMap, VecDeque},
    rc::Rc,
};

use slab::Slab;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum OrderType {
    GoodTillCancel,
    FillAndKill,
}
#[derive(Clone, Copy, Debug)]
pub enum Side {
    Buy,
    Sell,
}

type Price = i64;
type Quantity = u32;
type OrderId = u64;

pub struct LevelInfo {
    pub price: Price,
    pub quantity: Quantity,
}

type LevelInfos = Vec<LevelInfo>;
pub struct OrderBookLeveInfos {
    pub bids: LevelInfos,
    pub asks: LevelInfos,
}

impl OrderBookLeveInfos {
    pub fn getBids(&self) -> &LevelInfos {
        &self.bids
    }
    pub fn getAsks(&self) -> &LevelInfos {
        &self.asks
    }
}

#[derive(Debug)]
pub struct Order {
    order_type: OrderType,
    order_id: OrderId,
    side: Side,
    price: Price,
    intial_quantity: Quantity,
    remaining_quantity: Quantity,
}
impl Order {
    pub fn get_order_id(&self) -> OrderId {
        self.order_id
    }
    pub fn get_side(&self) -> Side {
        self.side
    }
    pub fn get_price(&self) -> Price {
        self.price
    }
    pub fn get_order_type(&self) -> OrderType {
        self.order_type
    }
    pub fn get_inital_quantity(&self) -> Quantity {
        self.intial_quantity
    }
    pub fn get_remaining_quantity(&self) -> Quantity {
        self.remaining_quantity
    }
    pub fn get_filled_quantity(&self) -> Quantity {
        self.intial_quantity - self.remaining_quantity
    }
    pub fn fill(&mut self, quantity: Quantity) {
        if quantity > self.get_remaining_quantity() {
            eprintln!(
                " Order {:#?} cannot be filled more than its remaining quantity. OrderID : {}",
                self, self.order_id
            );
            return;
        }
        self.remaining_quantity -= quantity;
    }

    pub fn is_filled(&self) -> bool {
        self.get_remaining_quantity() == 0
    }
}

type OrderPointer = Rc<RefCell<Order>>;
struct PriceLevel {
    orders: Slab<OrderPointer>,
    queue: VecDeque<usize>,
}
impl PriceLevel {
    fn new() -> Self {
        Self {
            orders: Slab::new(),
            queue: VecDeque::new(),
        }
    }
    fn insert(&mut self, order: OrderPointer) -> usize {
        let key = self.orders.insert(order);
        self.queue.push_back(key);
        key
    }
    fn remove(&mut self, key: usize) {
        self.orders.remove(key);
        self.queue.retain(|k| *k != key);
    }
    fn is_empty(&self) -> bool {
        self.orders.is_empty()
    }
    fn front(&self) -> Option<&OrderPointer> {
        let key = self.queue.front()?;
        Some(&self.orders[*key])
    }
    fn pop_front(&mut self) -> Option<OrderPointer> {
        let key = self.queue.pop_front()?;
        Some(self.orders.remove(key))
    }
}

#[derive(Debug)]
pub struct ModifyOrder {
    order_type: OrderType,
    order_id: OrderId,
    side: Side,
    price: Price,
    quantity: Quantity,
}

impl ModifyOrder {
    pub fn get_order_id(&self) -> OrderId {
        self.order_id
    }
    pub fn get_side(&self) -> Side {
        self.side
    }
    pub fn get_price(&self) -> Price {
        self.price
    }

    pub fn get_quantity(&self) -> Quantity {
        self.quantity
    }

    pub fn to_order_pointer(&self, order_type: OrderType) -> OrderPointer {
        let order = Order {
            order_type,
            side: self.get_side(),
            order_id: self.get_order_id(),
            price: self.get_price(),
            intial_quantity: self.get_quantity(),
            remaining_quantity: self.get_quantity(),
        };
        return Rc::new(RefCell::new(order));
    }
}

#[derive(Copy, Clone)]
struct TradeInfo {
    pub order_id: OrderId,
    pub price: Price,
    pub quantity: Quantity,
}

#[derive(Copy, Clone)]
struct Trade {
    pub bid_trade: TradeInfo,
    pub ask_trade: TradeInfo,
}

impl Trade {
    pub fn get_bid_trade(&self) -> TradeInfo {
        self.bid_trade
    }
    pub fn get_ask_trade(&self) -> TradeInfo {
        self.ask_trade
    }
}

type Trades = Vec<Trade>;

struct OrderEntry {
    pub order: OrderPointer,
    pub price: Price,
    pub side: Side,
    pub slab_key: usize,
}

struct OrderBook {
    asks: BTreeMap<Price, PriceLevel>,          // lowest price first
    bids: BTreeMap<Reverse<Price>, PriceLevel>, // higest price first
    orders: HashMap<OrderId, OrderEntry>,
}

impl OrderBook {
    pub fn new() -> Self {
        Self {
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
            orders: HashMap::new(),
        }
    }
    fn can_match(&self, side: Side, price: Price) -> bool {
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
    fn match_orders(&mut self) -> Trades {
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
                OrderType::GoodTillCancel => {},
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
                OrderType::GoodTillCancel => {},
            }
        }

        trades
    }

    pub fn add_order(&mut self, order: OrderPointer) -> Option<Trades> {
        let (order_type, order_id, order_side, order_price) = {
            (
                order.borrow().get_order_type(),
                order.borrow().get_order_id(),
                order.borrow().get_side(),
                order.borrow().get_price(),
            )
        };

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
                order,
                price: order_price,
                side: order_side,
                slab_key,
            },
        );

        Some(self.match_orders())
    }
    pub fn cancel_order(&mut self, order_id: OrderId) {
        let Some(entry) = self.orders.remove(&order_id) else {
            return;
        };

        match entry.side {
            Side::Buy => {
                let level = self.bids.get_mut(&Reverse(entry.price)).unwrap();
                level.remove(entry.slab_key);
                if level.is_empty() {
                    self.bids.remove(&Reverse(entry.price));
                }
            },
            Side::Sell => {
                let level = self.asks.get_mut(&entry.price).unwrap();
                level.remove(entry.slab_key);
                if level.is_empty() {
                    self.asks.remove(&entry.price);
                }
            },
        }
    }

    pub fn modify_order(&mut self, modify_order: ModifyOrder) -> Option<Trades> {
        if !self.orders.contains_key(&modify_order.order_id) {
            return None;
        }
        let exsisting_order = self.orders.get(&modify_order.order_id).unwrap();
        let order_type = exsisting_order.order.borrow().get_order_type();
        self.cancel_order(modify_order.order_id);
        return self.add_order(modify_order.to_order_pointer(order_type));
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_order(
        id: OrderId,
        side: Side,
        price: Price,
        qty: Quantity,
        order_type: OrderType,
    ) -> OrderPointer {
        Rc::new(RefCell::new(Order {
            order_type,
            order_id: id,
            side,
            price,
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
        assert_eq!(trades[0].bid_trade.price, 100); // higest bid price
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
}
