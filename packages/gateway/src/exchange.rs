use std::sync::{Arc, Mutex};

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use disruptor::RingBufferFull;
use domain::{ModifyOrder, NewOrder, OrderId, OrderType, Symbol};
use engine::commands::{CommandDispatcher, ExchangeCommand};

use crate::types::NewOrderReq;

/// Error returned when interacting with the exchange ring buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExchangeAccessError {
    /// Ring buffer is completely full and cannot accept commands without blocking.
    BufferFull,
    /// Shared mutex lock on the dispatcher was poisoned.
    LockPoisoned,
    /// Invalid order parameters or configuration.
    InvalidCommand(String),
}

impl std::fmt::Display for ExchangeAccessError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BufferFull => write!(f, "Exchange command ring buffer is full"),
            Self::LockPoisoned => write!(f, "Exchange dispatcher lock poisoned"),
            Self::InvalidCommand(msg) => write!(f, "Invalid exchange command: {msg}"),
        }
    }
}

impl std::error::Error for ExchangeAccessError {}

impl From<RingBufferFull> for ExchangeAccessError {
    fn from(_: RingBufferFull) -> Self {
        ExchangeAccessError::BufferFull
    }
}

impl IntoResponse for ExchangeAccessError {
    fn into_response(self) -> Response {
        match self {
            ExchangeAccessError::BufferFull => {
                (StatusCode::SERVICE_UNAVAILABLE, "Exchange ring buffer is full").into_response()
            },
            ExchangeAccessError::LockPoisoned => {
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal exchange lock error").into_response()
            },
            ExchangeAccessError::InvalidCommand(msg) => {
                (StatusCode::BAD_REQUEST, format!("Invalid command: {msg}")).into_response()
            },
        }
    }
}

// ---------------------------------------------------------------------------
// Standalone helper functions for direct access to exchange buffer
// ---------------------------------------------------------------------------

/// Non-blocking attempt to send an arbitrary command to the exchange ring buffer.
/// Returns the assigned sequence number on success, or `ExchangeAccessError::BufferFull`.
pub fn try_send_command(
    dispatcher: &mut CommandDispatcher,
    cmd: ExchangeCommand,
) -> Result<i64, ExchangeAccessError> {
    dispatcher.try_send(cmd).map_err(Into::into)
}

// /// Publishes an arbitrary command to the exchange ring buffer (blocking/spinning).
// pub fn publish_command(dispatcher: &mut CommandDispatcher, cmd: ExchangeCommand) {
//     dispatcher.publish(cmd);
// }

/// Helper function to submit a new order to the exchange buffer (non-blocking).
pub fn send_new_order(
    dispatcher: &mut CommandDispatcher,
    symbol: Symbol,
    order: NewOrder,
) -> Result<i64, ExchangeAccessError> {
    try_send_command(dispatcher, ExchangeCommand::AddNewOrder(symbol, order))
}

// /// Helper function to submit a new order to the exchange buffer (blocking/spinning).
// pub fn publish_new_order(dispatcher: &mut CommandDispatcher, symbol: Symbol, order: NewOrder) {
//     publish_command(dispatcher, ExchangeCommand::AddNewOrder(symbol, order));
// }

/// Helper function to cancel an order on the exchange buffer (non-blocking).
pub fn send_cancel_order(
    dispatcher: &mut CommandDispatcher,
    symbol: Symbol,
    order_id: OrderId,
) -> Result<i64, ExchangeAccessError> {
    try_send_command(dispatcher, ExchangeCommand::CancelOrder(symbol, order_id))
}

// /// Helper function to cancel an order on the exchange buffer (blocking/spinning).
// pub fn publish_cancel_order(dispatcher: &mut CommandDispatcher, symbol: Symbol, order_id: OrderId) {
//     publish_command(dispatcher, ExchangeCommand::CancelOrder(symbol, order_id));
// }

/// Helper function to modify an existing order on the exchange buffer (non-blocking).
pub fn send_modify_order(
    dispatcher: &mut CommandDispatcher,
    symbol: Symbol,
    modify: ModifyOrder,
) -> Result<i64, ExchangeAccessError> {
    try_send_command(dispatcher, ExchangeCommand::ModifyOrder(symbol, modify))
}

// /// Helper function to modify an existing order on the exchange buffer (blocking/spinning).
// pub fn publish_modify_order(dispatcher: &mut CommandDispatcher, symbol: Symbol, modify: ModifyOrder) {
//     publish_command(dispatcher, ExchangeCommand::ModifyOrder(symbol, modify));
// }

/// Helper function to prune expired orders on the exchange buffer (non-blocking).
pub fn send_prune_orders(
    dispatcher: &mut CommandDispatcher,
    symbol: Symbol,
    order_type: OrderType,
) -> Result<i64, ExchangeAccessError> {
    try_send_command(dispatcher, ExchangeCommand::PruneExpiredOrders(symbol, order_type))
}

/// Helper function to construct and send a new order from a `NewOrderReq` and assigned `OrderId`.
pub fn send_new_order_from_req(
    dispatcher: &mut CommandDispatcher,
    symbol: Symbol,
    req: &NewOrderReq,
    order_id: OrderId,
) -> Result<i64, ExchangeAccessError> {
    let new_order = NewOrder {
        order_id,
        user_id: req.user_id,
        asset_id: req.asset_id,
        price: req.price,
        quantity: req.quantity,
        order_type: req.order_type,
        side: req.side,
    };
    send_new_order(dispatcher, symbol, new_order)
}

// ---------------------------------------------------------------------------
// Thread-safe helper functions and ExchangeClient for AppState integration
// ---------------------------------------------------------------------------

/// Sends an exchange command to a shared Mutex-protected `CommandDispatcher` (non-blocking).
pub fn send_command_locked(
    dispatcher: &Mutex<CommandDispatcher>,
    cmd: ExchangeCommand,
) -> Result<i64, ExchangeAccessError> {
    let mut guard = dispatcher.lock().map_err(|_| ExchangeAccessError::LockPoisoned)?;
    try_send_command(&mut guard, cmd)
}

// /// Publishes an exchange command to a shared Mutex-protected `CommandDispatcher` (blocking).
// pub fn publish_command_locked(
//     dispatcher: &Mutex<CommandDispatcher>,
//     cmd: ExchangeCommand,
// ) -> Result<(), ExchangeAccessError> {
//     let mut guard = dispatcher.lock().map_err(|_| ExchangeAccessError::LockPoisoned)?;
//     guard.publish(cmd);
//     Ok(())
// }

/// Thread-safe client providing ergonomic access to the exchange buffer.
/// Can be stored directly on `AppState` (e.g. `state.cmd`) or passed to handlers.
#[derive(Clone)]
pub struct ExchangeClient {
    dispatcher: Arc<Mutex<CommandDispatcher>>,
}

impl ExchangeClient {
    pub fn new(dispatcher: CommandDispatcher) -> Self {
        Self {
            dispatcher: Arc::new(Mutex::new(dispatcher)),
        }
    }

    pub fn from_shared(dispatcher: Arc<Mutex<CommandDispatcher>>) -> Self {
        Self { dispatcher }
    }

    pub fn send_command(&self, cmd: ExchangeCommand) -> Result<i64, ExchangeAccessError> {
        send_command_locked(&self.dispatcher, cmd)
    }

    // pub fn publish_command(&self, cmd: ExchangeCommand) -> Result<(), ExchangeAccessError> {
    //     publish_command_locked(&self.dispatcher, cmd)
    // }

    pub fn add_new_order(&self, symbol: Symbol, order: NewOrder) -> Result<i64, ExchangeAccessError> {
        let mut guard = self.dispatcher.lock().map_err(|_| ExchangeAccessError::LockPoisoned)?;
        send_new_order(&mut guard, symbol, order)
    }

    pub fn cancel_order(&self, symbol: Symbol, order_id: OrderId) -> Result<i64, ExchangeAccessError> {
        let mut guard = self.dispatcher.lock().map_err(|_| ExchangeAccessError::LockPoisoned)?;
        send_cancel_order(&mut guard, symbol, order_id)
    }

    pub fn modify_order(&self, symbol: Symbol, modify: ModifyOrder) -> Result<i64, ExchangeAccessError> {
        let mut guard = self.dispatcher.lock().map_err(|_| ExchangeAccessError::LockPoisoned)?;
        send_modify_order(&mut guard, symbol, modify)
    }

    pub fn prune_expired_orders(&self, symbol: Symbol, order_type: OrderType) -> Result<i64, ExchangeAccessError> {
        let mut guard = self.dispatcher.lock().map_err(|_| ExchangeAccessError::LockPoisoned)?;
        send_prune_orders(&mut guard, symbol, order_type)
    }

    pub fn add_new_order_from_req(
        &self,
        symbol: Symbol,
        req: &NewOrderReq,
        order_id: OrderId,
    ) -> Result<i64, ExchangeAccessError> {
        let mut guard = self.dispatcher.lock().map_err(|_| ExchangeAccessError::LockPoisoned)?;
        send_new_order_from_req(&mut guard, symbol, req, order_id)
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

// #[cfg(test)]
// mod tests {
//     use disruptor::{BusySpin, build_multi_producer};
//     use domain::Side;
//     use engine::commands::{CommandConsumer, CommandEnvelope};
// 
//     use super::*;
// 
//     fn create_test_command_pipeline(buffer_size: usize) -> (CommandDispatcher, CommandConsumer) {
//         let builder = build_multi_producer(buffer_size, CommandEnvelope::empty, BusySpin);
//         let (poller, builder) = builder.new_event_poller();
//         let producer = builder.build();
//         (CommandDispatcher::new(producer), CommandConsumer::new(poller))
//     }
// 
//     fn sample_new_order(order_id: OrderId) -> NewOrder {
//         NewOrder {
//             order_id,
//             user_id: 42,
//             asset_id: 1,
//             price: Some(50_000),
//             quantity: 10,
//             order_type: OrderType::GoodTillCancel,
//             side: Side::Buy,
//         }
//     }
// 
//     #[test]
//     fn test_send_new_order_helper() {
//         let (mut dispatcher, mut consumer) = create_test_command_pipeline(64);
//         let order = sample_new_order(101);
// 
//         let seq = send_new_order(&mut dispatcher, Symbol::BtcUsdt, order.clone()).expect("send failed");
//         assert_eq!(seq, 0);
// 
//         let commands = consumer.poll().expect("poll failed");
//         assert_eq!(commands.len(), 1);
//         assert_eq!(commands[0], ExchangeCommand::AddNewOrder(Symbol::BtcUsdt, order));
//     }
// 
//     #[test]
//     fn test_send_cancel_order_helper() {
//         let (mut dispatcher, mut consumer) = create_test_command_pipeline(64);
// 
//         let seq = send_cancel_order(&mut dispatcher, Symbol::EthUsdt, 202).expect("cancel failed");
//         assert_eq!(seq, 0);
// 
//         let commands = consumer.poll().expect("poll failed");
//         assert_eq!(commands.len(), 1);
//         assert_eq!(commands[0], ExchangeCommand::CancelOrder(Symbol::EthUsdt, 202));
//     }
// 
//     #[test]
//     fn test_send_modify_order_helper() {
//         let (mut dispatcher, mut consumer) = create_test_command_pipeline(64);
//         let modify = ModifyOrder {
//             order_type: OrderType::GoodTillCancel,
//             order_id: 303,
//             side: Side::Sell,
//             price: 60_000,
//             quantity: 5,
//         };
// 
//         let seq = send_modify_order(&mut dispatcher, Symbol::BtcUsdt, modify).expect("modify failed");
//         assert_eq!(seq, 0);
// 
//         let commands = consumer.poll().expect("poll failed");
//         assert_eq!(commands.len(), 1);
//         assert_eq!(commands[0], ExchangeCommand::ModifyOrder(Symbol::BtcUsdt, modify));
//     }
// 
//     #[test]
//     fn test_send_prune_orders_helper() {
//         let (mut dispatcher, mut consumer) = create_test_command_pipeline(64);
// 
//         let seq = send_prune_orders(&mut dispatcher, Symbol::SolUsdt, OrderType::GoodForDay).expect("prune failed");
//         assert_eq!(seq, 0);
// 
//         let commands = consumer.poll().expect("poll failed");
//         assert_eq!(commands.len(), 1);
//         assert_eq!(
//             commands[0],
//             ExchangeCommand::PruneExpiredOrders(Symbol::SolUsdt, OrderType::GoodForDay)
//         );
//     }
// 
//     #[test]
//     fn test_send_new_order_from_req_helper() {
//         let (mut dispatcher, mut consumer) = create_test_command_pipeline(64);
//         let req = NewOrderReq {
//             user_id: 99,
//             asset_id: 2,
//             price: Some(3_000),
//             quantity: 20,
//             order_type: OrderType::GoodTillCancel,
//             side: Side::Buy,
//         };
// 
//         let seq = send_new_order_from_req(&mut dispatcher, Symbol::EthUsdt, &req, 555).expect("send from req failed");
//         assert_eq!(seq, 0);
// 
//         let commands = consumer.poll().expect("poll failed");
//         assert_eq!(commands.len(), 1);
//         match &commands[0] {
//             ExchangeCommand::AddNewOrder(sym, o) => {
//                 assert_eq!(*sym, Symbol::EthUsdt);
//                 assert_eq!(o.order_id, 555);
//                 assert_eq!(o.user_id, 99);
//                 assert_eq!(o.price, Some(3_000));
//             },
//             _ => panic!("expected AddNewOrder"),
//         }
//     }
// 
//     #[test]
//     fn test_exchange_client_wrapper() {
//         let (dispatcher, mut consumer) = create_test_command_pipeline(64);
//         let client = ExchangeClient::new(dispatcher);
// 
//         let order = sample_new_order(777);
//         client.add_new_order(Symbol::BtcUsdt, order.clone()).expect("add_new_order failed");
//         client.cancel_order(Symbol::BtcUsdt, 777).expect("cancel_order failed");
// 
//         let commands = consumer.poll().expect("poll failed");
//         assert_eq!(commands.len(), 2);
//         assert_eq!(commands[0], ExchangeCommand::AddNewOrder(Symbol::BtcUsdt, order));
//         assert_eq!(commands[1], ExchangeCommand::CancelOrder(Symbol::BtcUsdt, 777));
//     }
// 
//     #[test]
//     fn test_buffer_full_error() {
//         let (mut dispatcher, _consumer) = create_test_command_pipeline(64);
// 
//         // Fill ring buffer to capacity (64 slots)
//         for i in 0..64 {
//             let order = sample_new_order(i);
//             assert!(send_new_order(&mut dispatcher, Symbol::BtcUsdt, order).is_ok());
//         }
// 
//         // 65th send must return BufferFull
//         let overflow_order = sample_new_order(65);
//         let result = send_new_order(&mut dispatcher, Symbol::BtcUsdt, overflow_order);
//         assert_eq!(result, Err(ExchangeAccessError::BufferFull));
//     }
// }

