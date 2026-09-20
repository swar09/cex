use domain::{ModifyOrder, OrderId, Price, Quantity};

#[derive(Clone, Copy)]
pub enum OrderBookEvents {
    OrderMatched(OrderId, Price, Quantity), // orderid or orderpointer ?
    OrderCancelled(OrderId),
    OrderAdded(OrderId),
    OrderModified(ModifyOrder),
    OrderRejected(),
    OrderExpired(),
    OrderPartiallyFilled(),

    MarketOpened(),
    MarketClosed(),

    Error(),
}
