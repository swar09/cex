use std::{cell::RefCell, rc::Rc};

use serde::{Deserialize, Serialize};

use crate::{
    RemainingQty,
    types::{AssetId, OrderId, Price, Quantity, Side, UserId},
};

pub type OrderIds = Vec<OrderId>;
pub type ExpredOrderInfo = (OrderId, UserId, Price, Side, RemainingQty);
pub type ExpredOrders = Vec<ExpredOrderInfo>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderType {
    GoodTillCancel,
    FillAndKill,
    FillOrKill,
    GoodForDay,
    Market,
}

// elminate the heap alloc at the hot path
pub type OrderPointer = Rc<RefCell<Order>>;

#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
pub struct Order {
    pub order_id: OrderId,
    pub user_id: UserId,
    pub asset_id: AssetId,
    pub price: Option<Price>,
    pub initial_quantity: Quantity,
    pub remaining_quantity: Quantity,
    pub order_type: OrderType,
    pub side: Side,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NewOrder {
    pub order_id: OrderId,
    pub user_id: UserId,
    pub asset_id: AssetId,
    pub price: Option<Price>,
    pub quantity: Quantity,
    pub order_type: OrderType,
    pub side: Side,
}

impl Order {
    pub fn new(order_id: OrderId, user_id: UserId, asset_id: AssetId, side: Side, price: Price, quantity: Quantity, order_type: OrderType) -> Self {
        Self {
            order_id,
            user_id,
            asset_id,
            price: Some(price),
            initial_quantity: quantity,
            remaining_quantity: quantity,
            order_type,
            side,
        }
    }

    pub fn new_market_order(order_id: OrderId, user_id: UserId, asset_id: AssetId, side: Side, quantity: Quantity, _order_type: OrderType) -> Self {
        let order_type = OrderType::Market;
        let price = None;
        Self {
            order_id,
            user_id,
            asset_id,
            price,
            initial_quantity: quantity,
            remaining_quantity: quantity,
            order_type,
            side,
        }
    }

    pub fn get_order_id(&self) -> OrderId {
        self.order_id
    }
    pub fn get_user_id(&self) -> UserId {
        self.user_id
    }
    pub fn get_asset_id(&self) -> AssetId {
        self.asset_id
    }
    pub fn get_side(&self) -> Side {
        self.side
    }
    pub fn get_price(&self) -> Price {
        self.price.unwrap() // intentional 
    }
    pub fn get_order_type(&self) -> OrderType {
        self.order_type
    }
    pub fn get_initial_quantity(&self) -> Quantity {
        self.initial_quantity
    }
    pub fn get_remaining_quantity(&self) -> Quantity {
        self.remaining_quantity
    }
    pub fn get_filled_quantity(&self) -> Quantity {
        self.initial_quantity - self.remaining_quantity
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
    pub fn to_good_till_cancel(&mut self, price: Price) {
        self.order_type = OrderType::GoodTillCancel;
        self.price = Some(price);
    }
    pub fn apply_modification(&mut self, modify: &ModifyOrder) {
        self.order_type = modify.order_type;
        self.side = modify.side;
        self.price = Some(modify.price);
        self.initial_quantity = modify.quantity;
        self.remaining_quantity = modify.quantity;
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModifyOrder {
    pub order_type: OrderType,
    pub order_id: OrderId,
    pub side: Side,
    pub price: Price,
    pub quantity: Quantity,
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
            order_id: self.get_order_id(),
            user_id: 0,
            asset_id: 0,
            price: Some(self.get_price()),
            initial_quantity: self.get_quantity(),
            remaining_quantity: self.get_quantity(),
            order_type,
            side: self.get_side(),
        };
        Rc::new(RefCell::new(order))
    }
}

impl From<NewOrder> for Order {
    fn from(new_order: NewOrder) -> Self {
        Self {
            order_id: new_order.order_id,
            user_id: new_order.user_id,
            asset_id: new_order.asset_id,
            price: new_order.price,
            initial_quantity: new_order.quantity,
            remaining_quantity: new_order.quantity,
            order_type: new_order.order_type,
            side: new_order.side,
        }
    }
}
