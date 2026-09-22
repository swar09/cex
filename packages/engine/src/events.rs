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

struct EventDispatcher {
    pub producer: SingleProducer<ExchangeEventProducer, MultiConsumerBarrier>, // multi consumers configured
}

impl EventDispatcher {
    pub fn new(p: SingleProducer<ExchangeEventProducer, MultiConsumerBarrier>) -> Self {
        Self { producer: p }
    }
    pub fn publish(&mut self, event: OrderBookEvents) {
        self.producer.publish(|e| e.event = Some(event));
    }
}

struct EventConsumer {
    pub event_poller: EventPoller<ExchangeEventProducer, MultiConsumerBarrier>,
}
impl EventConsumer {
    pub fn new(p: EventPoller<ExchangeEventProducer, MultiConsumerBarrier>) -> Self {
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

fn create_test() {
    let event_factory = || ExchangeEventProducer { event: None };
    let mut builder = build_single_producer(4096, event_factory, BusySpin);
    let (event_poller_1, builder) = builder.new_event_poller();
    let (event_poller_2, builder) = builder.new_event_poller();
    let mut p = builder.build();
    let mut new_dispatcher = EventDispatcher::new(p);
    new_dispatcher.publish(OrderBookEvents::Error(23));
}
pub struct ExchangeEventProducer {
    pub event: Option<OrderBookEvents>,
}

fn test() {
    let event_factory = || ExchangeEventProducer { event: None };
    let mut builder = build_single_producer(1000, event_factory, BusySpin);
    let (event_poller_1, builder) = builder.new_event_poller();
    let (event_poller_2, builder) = builder.new_event_poller();
    let mut producer = builder.build();
}
