use serde::{Deserialize, Serialize};

use crate::{
    CancelReason, CancelledQty, MakerOrderId, MakerRemainingQty, MakerSide, MakerUserId, NewPrice, NewQty, OldPrice, OldQty, OrderId, OrderType,
    Price, Quantity, RejectReason, Side, TakerOrderId, TakerRemainingQty, TakerSide, TakerUserId, TradeId, UserId,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderPlacedEvent {
    pub order_id: OrderId,
    pub user_id: UserId,
    pub side: Side,
    pub price: Price,
    pub quantity: Quantity,
    pub order_type: OrderType,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TradeExecutedEvent {
    pub trade_id: TradeId,
    pub maker_order_id: MakerOrderId,
    pub taker_order_id: TakerOrderId,
    pub maker_user_id: MakerUserId,
    pub taker_user_id: TakerUserId,
    pub maker_side: MakerSide,
    pub taker_side: TakerSide,
    pub price: Price,
    pub quantity: Quantity,
    pub maker_remaining_qty: MakerRemainingQty,
    pub taker_remaining_qty: TakerRemainingQty,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderCancelledEvent {
    pub order_id: OrderId,
    pub user_id: UserId,
    pub side: Side,
    pub price: Price,
    pub cancelled_qty: CancelledQty,
    pub reason: CancelReason,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderModifiedEvent {
    pub order_id: OrderId,
    pub user_id: UserId,
    pub side: Side,
    pub old_price: OldPrice,
    pub new_price: NewPrice,
    pub old_qty: OldQty,
    pub new_qty: NewQty,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderRejectedEvent {
    pub order_id: OrderId,
    pub user_id: UserId,
    pub reason: RejectReason,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModifyOrderRejectedEvent {
    pub order_id: OrderId,
    pub user_id: UserId,
    pub side: Side,
    pub old_price: OldPrice,
    pub new_price: NewPrice,
    pub old_qty: OldQty,
    pub new_qty: NewQty,
    pub reason: RejectReason,
}
