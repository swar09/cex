use disruptor::{EventPoller, MultiConsumerBarrier, Polling, Producer, SingleProducer};
use domain::{ModifyOrder, OrderId, OrderIds, Price, Quantity, Symbol};

#[derive(Clone, PartialEq, Debug)]
pub enum OrderBookEvents {
    OrderMatched(Symbol, OrderId, Price, Quantity), // trade happen
    OrderCancelled(Symbol, OrderId),                // order cancelled by trader
    OrderAdded(Symbol, OrderId),                    // order added by trader
    OrderModified(Symbol, ModifyOrder),             // order modified by trader
    OrderRejected(Symbol, OrderId),                 // order rejected by exchange
    ModifyOrderRejected(Symbol, ModifyOrder),       // modify order req rejected by exchange
    // TODO : How to handle this events ?
    OrdersExpired(Symbol, OrderIds), // order expired
    OrderPartiallyFilled(Symbol),    // todo , order partially filled
    MarketOpened(Symbol),            // todo market opens
    MarketClosed(Symbol),            // todo market clone
    Error(u32),                      // error in order book
}

pub struct EventDispatcher {
    pub producer: SingleProducer<ExchangeEvents, MultiConsumerBarrier>, // multi consumers configured
}

impl EventDispatcher {
    pub fn new(p: SingleProducer<ExchangeEvents, MultiConsumerBarrier>) -> Self {
        Self { producer: p }
    }
    // can stall producer if buffer is full
    pub fn publish(&mut self, event: OrderBookEvents) {
        self.producer.publish(|e| e.event = Some(event));
    }
    // will return error if event is full no stalling or blocking
    pub fn try_send(&mut self, event: OrderBookEvents) -> Result<i64, disruptor::RingBufferFull> {
        self.producer.try_publish(|e| e.event = Some(event))
    }
}

// x-Consumer struct is part of the workers
struct EventConsumer {
    pub event_poller: EventPoller<ExchangeEvents, MultiConsumerBarrier>,
}
impl EventConsumer {
    pub fn new(p: EventPoller<ExchangeEvents, MultiConsumerBarrier>) -> Self {
        Self { event_poller: p }
    }
    pub fn poll(&mut self) {
        match self.event_poller.poll() {
            Ok(mut event_gaurd) => {
                for exchange_message in &mut event_gaurd {
                    let _event = exchange_message.event.as_ref().unwrap();
                    // process the event
                    // self.format
                    // self.publish_to_kaftka
                    // self.publish_to_websockets
                    // etc
                }
            },
            Err(Polling::NoEvents) => {},
            Err(Polling::Shutdown) => {},
        }
    }
}

pub struct ExchangeEvents {
    pub event: Option<OrderBookEvents>,
}
