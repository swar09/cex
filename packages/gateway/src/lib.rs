use std::sync::Arc;

use engine::events::OrderbookEventLog;
use tokio::sync::broadcast::Sender;

pub mod error;
pub mod handlers;
pub mod routers;
pub mod types;
pub mod websocket;

#[derive(Clone)]
pub struct AppState {
    pub tx: Sender<Arc<OrderbookEventLog>>, // use .subscribe() method to get rcv
}
