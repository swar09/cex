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
use tokio::sync::broadcast::{Receiver, Sender};

use crate::{AppState, middleware::AuthUser};

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    auth_user: AuthUser,
    State(state): State<AppState>,
) -> Response {
    ws.on_upgrade(move |socket| handle(socket, state, auth_user))
}

pub fn start_ws_broadcaster(
    mut event_rx: Receiver<Arc<OrderbookEventLog>>,
    ws_tx: Sender<Message>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            match event_rx.recv().await {
                Ok(event) => match bincode::serialize(&*event) {
                    Ok(bytes) => {
                        let message = Message::Binary(bytes.into());
                        let _ = ws_tx.send(message);
                    }
                    Err(e) => {
                        eprintln!("Failed to serialize orderbook event: {e}");
                    }
                },
                Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                    eprintln!("Orderbook event broadcaster lagged by {skipped} events");
                    continue;
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    break;
                }
            }
        }
    })
}

pub async fn handle(socket: WebSocket, state: AppState, _auth_user: AuthUser) {
    let (sender, receiver) = socket.split();
    let ws_rx = state.ws_tx.subscribe();

    let mut send_task = tokio::spawn(ws_sender(sender, ws_rx));
    let mut recv_task = tokio::spawn(ws_receiver(receiver));

    tokio::select! {
        _ = &mut send_task => {
            recv_task.abort();
        }
        _ = &mut recv_task => {
            send_task.abort();
        }
    }
}

pub async fn ws_sender(mut sender: SplitSink<WebSocket, Message>, mut ws_rx: Receiver<Message>) {
    loop {
        match ws_rx.recv().await {
            Ok(message) => {
                if sender.send(message).await.is_err() {
                    return;
                }
            }
            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                continue;
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                return;
            }
        }
    }
}

pub async fn ws_receiver(mut receiver: SplitStream<WebSocket>) {
    while let Some(msg_result) = receiver.next().await {
        match msg_result {
            Ok(Message::Close(_)) => break,
            Ok(Message::Ping(_)) => {},
            Ok(Message::Pong(_)) => {},
            Ok(Message::Text(_)) | Ok(Message::Binary(_)) => {},
            Err(_) => break,
        }
    }
}

#[cfg(test)]
mod tests {
    use domain::{OrderType, Side, Symbol};
    use engine::events::OrderbookEventType;
    use tokio::sync::broadcast;

    use super::*;

    #[tokio::test]
    async fn test_ws_broadcaster_serde_once_send_for_many() {
        let (tx, _) = broadcast::channel(16);
        let (ws_tx, _) = broadcast::channel(16);

        let broadcaster_handle = start_ws_broadcaster(tx.subscribe(), ws_tx.clone());

        let mut client1_rx = ws_tx.subscribe();
        let mut client2_rx = ws_tx.subscribe();

        let event = Arc::new(OrderbookEventLog {
            event_type: Some(OrderbookEventType::OrderPlaced),
            sequence_no: Some(101),
            symbol: Some(Symbol::BtcInr),
            order_id: Some(5001),
            price: Some(6_500_000),
            quantity: Some(5),
            order_type: Some(OrderType::GoodTillCancel),
            order_side: Some(Side::Buy),
            order_ids: None,
            error_code: None,
        });

        tx.send(event.clone()).expect("send event to tx");

        let msg1 = client1_rx.recv().await.expect("client 1 should receive message");
        let msg2 = client2_rx.recv().await.expect("client 2 should receive message");

        let bytes1 = match msg1 {
            Message::Binary(b) => b,
            _ => panic!("expected binary message"),
        };
        let bytes2 = match msg2 {
            Message::Binary(b) => b,
            _ => panic!("expected binary message"),
        };

        assert_eq!(bytes1, bytes2);

        let decoded1: OrderbookEventLog = bincode::deserialize(&bytes1).expect("deserialize client 1 message");
        let decoded2: OrderbookEventLog = bincode::deserialize(&bytes2).expect("deserialize client 2 message");

        assert_eq!(decoded1, *event);
        assert_eq!(decoded2, *event);

        broadcaster_handle.abort();
    }
}

