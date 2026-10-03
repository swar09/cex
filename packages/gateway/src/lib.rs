use std::sync::Arc;

use axum::extract::ws::Message;
use cache::Cache;
use engine::events::OrderbookEventLog;
use tokio::sync::broadcast::Sender;

pub mod error;
pub mod exchange;
pub mod handlers;
pub mod middleware;
pub mod routers;
pub mod types;
pub mod websocket;

#[derive(Clone)]
pub struct AppState {
    pub tx: Sender<Arc<OrderbookEventLog>>,
    pub ws_tx: Sender<Message>,
    pub cache: Arc<Cache>,
    pub cmd: exchange::ExchangeClient,
}

impl AppState {
    pub fn new(
        tx: Sender<Arc<OrderbookEventLog>>,
        cache: Arc<Cache>,
        cmd: exchange::ExchangeClient,
        ws_capacity: usize,
    ) -> Self {
        let (ws_tx, _) = tokio::sync::broadcast::channel(ws_capacity);
        websocket::start_ws_broadcaster(tx.subscribe(), ws_tx.clone());
        Self {
            tx,
            ws_tx,
            cache,
            cmd,
        }
    }
}
