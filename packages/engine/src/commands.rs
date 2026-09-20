use domain::{ModifyOrder, NewOrder, OrderId, OrderType, Symbol};

pub enum ExchangeCommand {
    AddNewOrder(Symbol, NewOrder),
    CancelOrder(Symbol, OrderId),
    ModifyOrder(Symbol, ModifyOrder),
    PruneExpiredOrders(Symbol, OrderType),
    /* prune good for day :)
     * TODO : add new commands and write tests for all */
}

impl ExchangeCommand {
    // TODO : impl `ExchangeCommand`
}
