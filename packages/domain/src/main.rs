use rand::{Rng, RngExt};
use std::cmp::Reverse;
use std::{
    cell::RefCell,
    cmp::min,
    collections::{BTreeMap, HashMap, LinkedList},
    rc::Rc,
};

#[derive(Clone, Copy, Debug)]
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
    pub fn order_type(&self) -> OrderType {
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
type OrderPointers = LinkedList<OrderPointer>;

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

    pub fn to_order_pointer(&self, order_type: OrderType) -> Rc<ModifyOrder> {
        Rc::new(ModifyOrder {
            order_type,
            order_id: self.get_order_id(),
            side: self.get_side(),
            price: self.get_price(),
            quantity: self.get_quantity(),
        })
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
}

struct OrderBook {
    bids: BTreeMap<Reverse<Price>, OrderPointers>, // higest price first
    asks: BTreeMap<Price, OrderPointers>,          // lowest price first
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
                    let (_, order_pointer_bid) = self.bids.first_key_value().unwrap();
                    let (_, order_pointer_ask) = self.asks.first_key_value().unwrap();

                    let mut bid = order_pointer_bid.front().unwrap().borrow_mut();
                    let mut ask = order_pointer_ask.front().unwrap().borrow_mut();

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
                    self.bids.pop_first();
                    self.orders.remove(&bid_order_id);
                }

                if ask_filled {
                    self.asks.pop_first();
                    self.orders.remove(&ask_order_id);
                }
            }
        }

        if !self.bids.is_empty() {
            todo!()
        }
        if !self.asks.is_empty() {
            todo!()
        }

        trades
    }
    // fn () {}
}

fn main() {}
