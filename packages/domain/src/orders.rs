use std::{cell::RefCell, rc::Rc};

use crate::types::{OrderId, Price, Quantity, Side};

pub type OrderIds = Vec<OrderId>;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum OrderType {
    GoodTillCancel,
    FillAndKill,
    FillOrKill,
    GoodForDay,
    Market,
}

pub type OrderPointer = Rc<RefCell<Order>>;

#[derive(Debug)]
pub struct Order {
    pub order_type: OrderType,
    pub order_id: OrderId,
    pub side: Side,
    pub price: Option<Price>,
    pub intial_quantity: Quantity,
    pub remaining_quantity: Quantity,
}

#[derive(Debug)]
pub struct NewOrder {
    pub order_type: OrderType,
    pub order_id: OrderId,
    pub side: Side,
    pub price: Option<Price>,
    pub quantity: Quantity,
}

impl Order {
    pub fn new(order_id: OrderId, side: Side, price: Price, quantity: Quantity, order_type: OrderType) -> Self {
        Self {
            order_type,
            order_id,
            side,
            price: Some(price),
            intial_quantity: quantity,
            remaining_quantity: quantity,
        }
    }

    pub fn new_market_order(order_id: OrderId, side: Side, quantity: Quantity, _order_type: OrderType) -> Self {
        let order_type = OrderType::Market;
        let price = None;
        Self {
            order_type,
            order_id,
            side,
            price,
            intial_quantity: quantity,
            remaining_quantity: quantity,
        }
    }

    pub fn get_order_id(&self) -> OrderId {
        self.order_id
    }
    pub fn get_side(&self) -> Side {
        self.side
    }
    pub fn get_price(&self) -> Price {
        self.price.unwrap()
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
    pub fn to_good_till_cancel(&mut self, price: Price) {
        self.order_type = OrderType::GoodTillCancel;
        self.price = Some(price);
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
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
            order_type,
            side: self.get_side(),
            order_id: self.get_order_id(),
            price: Some(self.get_price()),
            intial_quantity: self.get_quantity(),
            remaining_quantity: self.get_quantity(),
        };
        Rc::new(RefCell::new(order))
    }
}
