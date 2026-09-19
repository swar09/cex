use crate::{OrderId, Price, Quantity};

#[derive(thiserror::Error, Debug, Clone)]
pub enum OrderError {
    #[error("order_id {0} zero price / invalid price")]
    ZeroPrice(Price),
    #[error("order_id {0} invalid order")]
    InvalidOrder(OrderId),
    #[error("quantity {0} invalid qunatity")]
    ZeroQuantity(Quantity),
    #[error("order_id {0} order not found")]
    OrderNotFound(OrderId),
    #[error("order_id {0} duplicate order")]
    DuplicateOrder(OrderId),
}
