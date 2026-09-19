use domain::{ModifyOrder, NewOrder, OrderId, Symbol};

pub enum ExchangeCommand {
    AddNewOrder(Symbol, NewOrder),
    CancelOrder(Symbol, OrderId),
    ModifyOrder(Symbol, ModifyOrder),
}
