use std::sync::Arc;

use axum::{
    extract::{
        State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    response::Response,
};
use engine::events::OrderbookEventLog;
use futures_util::{
    sink::SinkExt,
    stream::{SplitSink, SplitStream, StreamExt},
};
use tokio::sync::broadcast::Receiver;

use crate::AppState;

// also add auth middleware here
// auth once and get persistent connection using webscokets

pub async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> Response {
    ws.on_upgrade(|socket| handle(socket, state))
}
pub async fn handle(socket: WebSocket, state: AppState) {
    let (sender, receiver) = socket.split();
    tokio::spawn(ws_receiver(receiver)); // receiver
    tokio::spawn(ws_sender(sender, state.tx.subscribe())); // sender 
}

pub async fn ws_sender(mut sender: SplitSink<WebSocket, Message>, mut tx: Receiver<Arc<OrderbookEventLog>>) {
    loop {
        match tx.recv().await {
            Ok(event) => {
                // match with clients requested symbols then
                // serialize and send message to client
                let payload_bytes = serde_json::to_vec(&*event.clone()).unwrap(); // handle error later
                let message = Message::Binary(payload_bytes.into());
                if sender.send(message).await.is_err() {
                    // if error then client disconnected
                    return;
                }
            },
            Err(e) => {
                eprintln!("{e}");
            },
        }
    }
}
pub async fn ws_receiver(_receiver: SplitStream<WebSocket>) {
    loop {
        // receive the clients req in real time and respond to it
        // may req to change the symbols or stop
    }
}
// health check and continuous ping function
pub async fn ws_ping() {}
