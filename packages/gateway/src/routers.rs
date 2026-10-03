use axum::{
    Router,
    routing::{get, post},
};

use crate::{
    AppState,
    handlers::{cancel_order, create_new_order, get_order_by_id, health_check, login, logout, modify_order},
    websocket::ws_handler,
};

pub async fn v1_auth_routes() -> Router<AppState> {
    Router::<AppState>::new().route("/login", post(login)).route("/logout", post(logout))
}
pub async fn v1_order_routes() -> Router<AppState> {
    let router = Router::new()
        .route("/order", get(get_order_by_id).post(create_new_order))
        .route("/order/cancel", post(cancel_order))
        .route("/order/modify", post(modify_order));

    router
}
pub async fn v1_health_routes() -> Router<AppState> {
    Router::new().route("/health", get(health_check))
}
pub async fn v1_ws_routes() -> Router<AppState> {
    Router::new().route("/ws", get(ws_handler))
}

