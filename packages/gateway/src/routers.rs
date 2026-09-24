use axum::{
    Router,
    routing::{get, post},
};

use crate::{
    AppState,
    handlers::{cancel_order, create_new_order, get_order_by_id, health_check, login, logout, modify_order},
};

pub async fn v1_auth_routes() -> Router<AppState> {
    
    Router::<AppState>::new()
        .route("/login", post(login))
        .route("/logout", post(logout))
}
pub async fn v1_order_routes() -> Router<AppState> {
    let router = Router::new()
        .route("/order", get(get_order_by_id))
        .route("/order", post(create_new_order))
        .route("/order/cancel", post(cancel_order))
        .route("/order/modify", post(modify_order));
    // .route("/order", method_router);

    router
}
pub async fn v1_health_routes() -> Router<AppState> {
    
    Router::new().route("/health", get(health_check))
}
