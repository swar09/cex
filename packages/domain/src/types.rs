use serde::Serialize;
use smallvec::SmallVec;

pub type AssetId = u64;
pub type Price = u64;
pub type Quantity = u32;
pub type OrderId = u64;
pub type UserId = u64;

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub enum Side {
    Buy,
    Sell,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct TradeInfo {
    pub order_id: OrderId,
    pub user_id: UserId,
    pub price: Price,
    pub quantity: Quantity,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Trade {
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

pub type Trades = SmallVec<[Trade; 8]>;
