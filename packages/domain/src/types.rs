pub type Price = i64;
pub type Quantity = u32;
pub type OrderId = u64;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Side {
    Buy,
    Sell,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct TradeInfo {
    pub order_id: OrderId,
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

pub type Trades = Vec<Trade>;

