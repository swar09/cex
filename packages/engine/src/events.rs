use domain::{OrderId, Price, Quantity};

#[derive(Clone, Copy)]
pub enum OrderBookEvents {
    OrderMatched(OrderId, Price, Quantity), // orderid or orderpointer ?
    OrderCancelled(),
    OrderAdded(),
    OrderModified(),
    OrderRejected(),
    OrderExpired(),
    OrderPartiallyFilled(),

    MarketOpened(),
    MarketClosed(),

    Error(),
}
