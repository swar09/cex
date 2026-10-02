use domain::{AssetId, OrderId, OrderType, Price, Quantity, Side, UserId};

pub struct NewOrderReq {
    pub user_id: UserId,
    pub asset_id: AssetId,
    pub price: Option<Price>,
    pub quantity: Quantity,
    pub order_type: OrderType,
    pub side: Side,
}

pub fn ext_order_id_generator() -> OrderId {
    1111
}
