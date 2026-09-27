use axum::extract::State;

use crate::AppState;

pub async fn ws_handler(State(state): State<AppState>) {
    tokio::spawn(ws_reciver()); // reciver
    tokio::spawn(ws_sender()); // sender 
}

pub async fn ws_sender() {}
pub async fn ws_reciver() {}
pub async fn ws_ping() {}
