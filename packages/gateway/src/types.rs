use domain::{AssetId, ModifyOrder, OrderId, OrderType, Price, Quantity, Side, Symbol, UserId};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct NewOrderReq {
    pub symbol: Symbol,
    pub user_id: UserId,
    pub asset_id: AssetId,
    pub price: Option<Price>,
    pub quantity: Quantity,
    pub order_type: OrderType,
    pub side: Side,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GetOrderQuery {
    pub order_id: OrderId,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CancelOrderReq {
    pub symbol: Symbol,
    pub order_id: OrderId,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ModifyOrderReq {
    pub symbol: Symbol,
    pub modify: ModifyOrder,
}
