use disruptor::{EventPoller, MultiProducer, MultiProducerBarrier, Producer, SingleConsumerBarrier};
use domain::{ModifyOrder, NewOrder, OrderId, OrderType, Symbol};

#[derive(Debug, Clone, PartialEq)]
pub enum ExchangeCommand {
    AddNewOrder(Symbol, NewOrder),
    CancelOrder(Symbol, OrderId),
    ModifyOrder(Symbol, ModifyOrder),
    PruneExpiredOrders(Symbol, OrderType),
}

impl ExchangeCommand {
    pub fn symbol(&self) -> Symbol {
        match self {
            Self::AddNewOrder(symbol, _)
            | Self::CancelOrder(symbol, _)
            | Self::ModifyOrder(symbol, _)
            | Self::PruneExpiredOrders(symbol, _) => *symbol,
        }
    }

    pub fn new_order(symbol: Symbol, order: NewOrder) -> Self {
        Self::AddNewOrder(symbol, order)
    }

    pub fn cancel_order(symbol: Symbol, order_id: OrderId) -> Self {
        Self::CancelOrder(symbol, order_id)
    }

    pub fn modify_order(symbol: Symbol, modify: ModifyOrder) -> Self {
        Self::ModifyOrder(symbol, modify)
    }

    pub fn prune_expired_orders(symbol: Symbol, order_type: OrderType) -> Self {
        Self::PruneExpiredOrders(symbol, order_type)
    }
}

/// ring buffer slot containing an optional exchange command.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CommandEnvelope {
    pub command: Option<ExchangeCommand>,
}

impl CommandEnvelope {
    pub fn new(command: ExchangeCommand) -> Self {
        Self { command: Some(command) }
    }

    pub fn empty() -> Self {
        Self { command: None }
    }
}

pub struct CommandDispatcher {
    pub producer: MultiProducer<CommandEnvelope, SingleConsumerBarrier>,
}

impl CommandDispatcher {
    pub fn new(producer: MultiProducer<CommandEnvelope, SingleConsumerBarrier>) -> Self {
        Self { producer }
    }

    // can stall producer if ring buffer is full
    pub fn publish(&mut self, cmd: ExchangeCommand) {
        self.producer.publish(|slot| slot.command = Some(cmd));
    }

    // returns error if ring buffer is full (non-blocking)
    pub fn try_send(&mut self, cmd: ExchangeCommand) -> Result<i64, disruptor::RingBufferFull> {
        self.producer.try_publish(|slot| slot.command = Some(cmd))
    }
}

pub struct CommandConsumer {
    pub poller: EventPoller<CommandEnvelope, MultiProducerBarrier>,
}

impl CommandConsumer {
    pub fn new(poller: EventPoller<CommandEnvelope, MultiProducerBarrier>) -> Self {
        Self { poller }
    }

    pub fn poll(&mut self) -> Result<Vec<ExchangeCommand>, disruptor::Polling> {
        match self.poller.poll() {
            Ok(mut guard) => {
                let mut commands = Vec::new();
                for slot in &mut guard {
                    if let Some(cmd) = &slot.command {
                        commands.push(cmd.clone());
                    }
                }
                Ok(commands)
            },
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use disruptor::{BusySpin, Polling, build_multi_producer};
    use domain::{Side, Symbol};

    use super::*;

    fn create_command_pipeline(buffer_size: usize) -> (CommandDispatcher, CommandConsumer) {
        let builder = build_multi_producer(buffer_size, CommandEnvelope::empty, BusySpin);
        let (poller, builder) = builder.new_event_poller();
        let producer = builder.build();
        (CommandDispatcher::new(producer), CommandConsumer::new(poller))
    }

    fn sample_new_order(order_id: OrderId) -> NewOrder {
        NewOrder {
            order_id,
            user_id: 1,
            asset_id: 1,
            order_type: OrderType::GoodTillCancel,
            side: Side::Buy,
            price: Some(100),
            quantity: 5,
        }
    }

    fn sample_modify_order(order_id: OrderId) -> ModifyOrder {
        ModifyOrder {
            order_type: OrderType::GoodTillCancel,
            order_id,
            side: Side::Buy,
            price: 105,
            quantity: 10,
        }
    }

    #[test]
    fn test_exchange_command_symbol() {
        let add = ExchangeCommand::new_order(Symbol::BtcInr, sample_new_order(1));
        assert_eq!(add.symbol(), Symbol::BtcInr);

        let cancel = ExchangeCommand::cancel_order(Symbol::EthInr, 2);
        assert_eq!(cancel.symbol(), Symbol::EthInr);

        let modify = ExchangeCommand::modify_order(Symbol::SolUsdt, sample_modify_order(3));
        assert_eq!(modify.symbol(), Symbol::SolUsdt);

        let prune = ExchangeCommand::prune_expired_orders(Symbol::UsdtInr, OrderType::GoodForDay);
        assert_eq!(prune.symbol(), Symbol::UsdtInr);
    }

    #[test]
    fn test_command_envelope_constructors() {
        let empty = CommandEnvelope::empty();
        assert_eq!(empty.command, None);

        let default_env = CommandEnvelope::default();
        assert_eq!(default_env.command, None);

        let cmd = ExchangeCommand::cancel_order(Symbol::BtcUsdt, 42);
        let with_cmd = CommandEnvelope::new(cmd.clone());
        assert_eq!(with_cmd.command, Some(cmd));
    }

    #[test]
    fn test_command_dispatcher_publish_and_poll() {
        let (mut dispatcher, mut consumer) = create_command_pipeline(64);

        // before any publish, polling returns NoEvents
        assert_eq!(consumer.poll().err(), Some(Polling::NoEvents));

        let cmd1 = ExchangeCommand::new_order(Symbol::BtcInr, sample_new_order(1));
        let cmd2 = ExchangeCommand::cancel_order(Symbol::BtcInr, 1);

        dispatcher.publish(cmd1.clone());
        dispatcher.publish(cmd2.clone());

        let received = consumer.poll().expect("should poll commands");
        assert_eq!(received, vec![cmd1, cmd2]);
    }

    #[test]
    fn test_command_dispatcher_try_send_success() {
        let (mut dispatcher, mut consumer) = create_command_pipeline(64);

        let cmd = ExchangeCommand::modify_order(Symbol::EthUsdt, sample_modify_order(10));
        let result = dispatcher.try_send(cmd.clone());
        assert!(result.is_ok());

        let received = consumer.poll().expect("should poll command");
        assert_eq!(received, vec![cmd]);
    }

    #[test]
    fn test_command_dispatcher_try_send_buffer_full() {
        let (mut dispatcher, _consumer) = create_command_pipeline(64);

        // fill buffer to capacity
        for i in 0..64 {
            let cmd = ExchangeCommand::cancel_order(Symbol::BtcInr, i);
            assert!(dispatcher.try_send(cmd).is_ok());
        }

        // 65 th send must fail with RingBufferFull
        let overflow = dispatcher.try_send(ExchangeCommand::cancel_order(Symbol::BtcInr, 999));
        assert_eq!(overflow, Err(disruptor::RingBufferFull));
    }

    #[test]
    fn test_command_variants_clone_and_equality() {
        let cmd1 = ExchangeCommand::new_order(Symbol::BtcInr, sample_new_order(1));
        let cmd2 = cmd1.clone();
        assert_eq!(cmd1, cmd2);

        let cmd3 = ExchangeCommand::cancel_order(Symbol::BtcInr, 1);
        assert_ne!(cmd1, cmd3);
    }
}
