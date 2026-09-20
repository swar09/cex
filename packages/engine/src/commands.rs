use domain::{ModifyOrder, NewOrder, OrderId, Symbol};

pub enum ExchangeCommand {
    AddNewOrder(Symbol, NewOrder),
    CancelOrder(Symbol, OrderId),
    ModifyOrder(Symbol, ModifyOrder),
    // TODO : add new commands and write tests for all
}

impl ExchangeCommand {
    // TODO : impl `ExchangeCommand`
}
