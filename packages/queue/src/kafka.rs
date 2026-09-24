use rdkafka::{ClientConfig, error::KafkaError, producer::BaseProducer};

pub struct KafkaClient {
    pub producer: BaseProducer,
    pub config: ClientConfig,
}

impl KafkaClient {
    pub fn new(config: ClientConfig) -> Self {
        let producer: BaseProducer = config.create().expect("Cannot create producer");
        Self { producer, config }
    }

    pub fn new_producer(&self) -> Result<BaseProducer, KafkaError> {
        self.config.create()
    }
}
