#[derive(Clone, Copy)]
pub enum OrderBookEvents {
    OrderMatched(), // orderid or orderpointer ?
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
