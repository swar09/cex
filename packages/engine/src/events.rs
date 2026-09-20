use domain::{ModifyOrder, OrderId, Price, Quantity, Symbol};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum OrderBookEvents {
    OrderMatched(Symbol, OrderId, Price, Quantity), // orderid or orderpointer ?
    OrderCancelled(Symbol, OrderId),
    OrderAdded(Symbol, OrderId),
    OrderModified(Symbol, ModifyOrder),
    OrderRejected(Symbol, OrderId),
    ModifyOrderRejected(Symbol, ModifyOrder),
    // TODO : How to handle this events
    OrderExpired(Symbol),
    OrderPartiallyFilled(Symbol),
    MarketOpened(Symbol),
    MarketClosed(Symbol),
    Error(),
}
