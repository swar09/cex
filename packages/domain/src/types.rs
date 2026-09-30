use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

pub type AssetId = u64;
pub type UserId = u64;
pub type OrderId = u64;
pub type TradeId = u64;

pub type Price = u64;
pub type Quantity = u32;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Side {
    Buy,
    Sell,
}

pub type MakerSide = Side;
pub type TakerSide = Side;

pub type OldPrice = Price;
pub type NewPrice = Price;
pub type MakerFee = Price;
pub type TakerFee = Price;

pub type OldQty = Quantity;
pub type NewQty = Quantity;
pub type RemainingQty = Quantity;
pub type CancelledQty = Quantity;
pub type MakerRemainingQty = Quantity;
pub type TakerRemainingQty = Quantity;

pub type MakerOrderId = OrderId;
pub type TakerOrderId = OrderId;
pub type MakerUserId = UserId;
pub type TakerUserId = UserId;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TradeInfo {
    pub order_id: OrderId,
    pub user_id: UserId,
    pub price: Price,
    pub quantity: Quantity,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
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

/// Pre-allocated small vector for trades on the stack; spills to heap if
/// exceeding capacity.
pub type Trades = SmallVec<[Trade; 64]>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CancelReason {
    UserRequested,
    FAKRemainder,
    SelfTradePrevention,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RejectReason {
    InsufficientBalance,
    AccountFrozen,
    PostOnlyWouldCross,
    MarketClosed,
    NoLiquidity,
}
