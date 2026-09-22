use disruptor::{
    BusySpin, EventPoller, MultiConsumerBarrier, Polling, Producer, SingleProducer, build_single_producer,
};
use domain::{ModifyOrder, OrderId, OrderIds, Price, Quantity, Symbol};

#[derive(Clone, PartialEq, Debug)]
pub enum OrderBookEvents {
    OrderMatched(Symbol, OrderId, Price, Quantity), // orderid or orderpointer ?
    OrderCancelled(Symbol, OrderId),
    OrderAdded(Symbol, OrderId),
    OrderModified(Symbol, ModifyOrder),
    OrderRejected(Symbol, OrderId),
    ModifyOrderRejected(Symbol, ModifyOrder),
    // TODO : How to handle this events
    OrdersExpired(Symbol, OrderIds),
    OrderPartiallyFilled(Symbol),
    MarketOpened(Symbol),
    MarketClosed(Symbol),
    Error(u32),
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
                    let event = exchange_message.event.as_ref().unwrap();
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
