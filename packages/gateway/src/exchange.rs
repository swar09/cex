use std::{
    cell::RefCell,
    sync::atomic::{AtomicU64, Ordering},
};

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use disruptor::RingBufferFull;
use domain::{ModifyOrder, NewOrder, OrderId, Symbol};
use engine::commands::{CommandDispatcher, ExchangeCommand};

static ORDER_ID_COUNTER: AtomicU64 = AtomicU64::new(0);

pub fn ext_order_id_generator() -> OrderId {
    ORDER_ID_COUNTER.fetch_add(1, Ordering::Relaxed)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExchangeAccessError {
    BufferFull,
    InvalidCommand(String),
}

impl std::fmt::Display for ExchangeAccessError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BufferFull => write!(f, "Exchange command ring buffer is full"),
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
            ExchangeAccessError::BufferFull => (StatusCode::SERVICE_UNAVAILABLE, "Exchange ring buffer is full").into_response(),
            ExchangeAccessError::InvalidCommand(msg) => (StatusCode::BAD_REQUEST, format!("Invalid command: {msg}")).into_response(),
        }
    }
}

pub fn try_send_command(dispatcher: &mut CommandDispatcher, cmd: ExchangeCommand) -> Result<i64, ExchangeAccessError> {
    dispatcher.try_send(cmd).map_err(Into::into)
}

thread_local! {
    static LOCAL_DISPATCHER: RefCell<Option<CommandDispatcher>> = const { RefCell::new(None) };
}

#[inline]
pub fn send_to_exchange(template: &CommandDispatcher, cmd: ExchangeCommand) -> Result<i64, ExchangeAccessError> {
    LOCAL_DISPATCHER.with(|cell| {
        let mut opt = cell.borrow_mut();
        let dispatcher = opt.get_or_insert_with(|| template.clone());
        try_send_command(dispatcher, cmd)
    })
}

#[derive(Clone)]
pub struct ExchangeClient {
    pub template: CommandDispatcher,
}

impl ExchangeClient {
    pub fn new(template: CommandDispatcher) -> Self {
        Self { template }
    }

    #[inline]
    pub fn send(&self, cmd: ExchangeCommand) -> Result<i64, ExchangeAccessError> {
        send_to_exchange(&self.template, cmd)
    }

    #[inline]
    pub fn add_new_order(&self, symbol: Symbol, order: NewOrder) -> Result<i64, ExchangeAccessError> {
        self.send(ExchangeCommand::AddNewOrder(symbol, order))
    }

    #[inline]
    pub fn cancel_order(&self, symbol: Symbol, order_id: OrderId) -> Result<i64, ExchangeAccessError> {
        self.send(ExchangeCommand::CancelOrder(symbol, order_id))
    }

    #[inline]
    pub fn modify_order(&self, symbol: Symbol, modify: ModifyOrder) -> Result<i64, ExchangeAccessError> {
        self.send(ExchangeCommand::ModifyOrder(symbol, modify))
    }
}

#[cfg(test)]
mod tests {
    use disruptor::{BusySpin, build_multi_producer};
    use domain::{OrderType, Side};
    use engine::commands::{CommandConsumer, CommandEnvelope};

    use super::*;

    fn create_test_pipeline(buffer_size: usize) -> (CommandDispatcher, CommandConsumer) {
        let builder = build_multi_producer(buffer_size, CommandEnvelope::empty, BusySpin);
        let (poller, builder) = builder.new_event_poller();
        let producer = builder.build();
        (CommandDispatcher::new(producer), CommandConsumer::new(poller))
    }

    fn sample_new_order(order_id: OrderId) -> NewOrder {
        NewOrder {
            order_id,
            user_id: 1,
            asset_id: 2,
            price: Some(50_000),
            quantity: 10,
            order_type: OrderType::GoodTillCancel,
            side: Side::Buy,
        }
    }

    #[test]
    fn test_try_send_command() {
        let (mut dispatcher, mut consumer) = create_test_pipeline(64);
        let order = sample_new_order(101);

        let seq = try_send_command(&mut dispatcher, ExchangeCommand::new_order(Symbol::BtcUsdt, order.clone())).unwrap();
        assert_eq!(seq, 0);

        let commands = consumer.poll().unwrap();
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0], ExchangeCommand::AddNewOrder(Symbol::BtcUsdt, order));
    }

    #[test]
    fn test_send_to_exchange() {
        let (dispatcher, mut consumer) = create_test_pipeline(64);
        let order = sample_new_order(201);

        let seq = send_to_exchange(&dispatcher, ExchangeCommand::new_order(Symbol::EthUsdt, order.clone())).unwrap();
        assert_eq!(seq, 0);

        let cancel_seq = send_to_exchange(&dispatcher, ExchangeCommand::cancel_order(Symbol::EthUsdt, 201)).unwrap();
        assert_eq!(cancel_seq, 1);

        let commands = consumer.poll().unwrap();
        assert_eq!(commands.len(), 2);
        assert_eq!(commands[0], ExchangeCommand::AddNewOrder(Symbol::EthUsdt, order));
        assert_eq!(commands[1], ExchangeCommand::CancelOrder(Symbol::EthUsdt, 201));
    }

    #[test]
    fn test_exchange_client() {
        let (dispatcher, mut consumer) = create_test_pipeline(64);
        let client = ExchangeClient::new(dispatcher);

        let order = sample_new_order(301);
        client.add_new_order(Symbol::SolUsdt, order.clone()).unwrap();
        client.cancel_order(Symbol::SolUsdt, 301).unwrap();

        let modify = ModifyOrder {
            order_type: OrderType::GoodTillCancel,
            order_id: 301,
            side: Side::Sell,
            price: 150,
            quantity: 5,
        };
        client.modify_order(Symbol::SolUsdt, modify).unwrap();

        let commands = consumer.poll().unwrap();
        assert_eq!(commands.len(), 3);
        assert_eq!(commands[0], ExchangeCommand::AddNewOrder(Symbol::SolUsdt, order));
        assert_eq!(commands[1], ExchangeCommand::CancelOrder(Symbol::SolUsdt, 301));
        assert_eq!(commands[2], ExchangeCommand::ModifyOrder(Symbol::SolUsdt, modify));
    }

    #[test]
    fn test_buffer_full_error() {
        let (mut dispatcher, _consumer) = create_test_pipeline(64);

        for i in 0..64 {
            let order = sample_new_order(i);
            assert!(try_send_command(&mut dispatcher, ExchangeCommand::new_order(Symbol::BtcUsdt, order)).is_ok());
        }

        let overflow_order = sample_new_order(65);
        let result = try_send_command(&mut dispatcher, ExchangeCommand::new_order(Symbol::BtcUsdt, overflow_order));
        assert_eq!(result, Err(ExchangeAccessError::BufferFull));
    }
}
