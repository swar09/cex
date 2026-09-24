use axum::{
    Json,
    extract::State,
    response::{IntoResponse, Response},
};

use crate::AppState;

pub async fn health_check() -> Response {
    Json("ok").into_response()
}

pub async fn create_new_order(State(_state): State<AppState>) -> Response {
    todo!()
}
pub async fn get_order_by_id(State(_state): State<AppState>) -> Response {
    todo!()
}
pub async fn cancel_order(State(_state): State<AppState>) -> Response {
    todo!()
}
pub async fn modify_order(State(_state): State<AppState>) -> Response {
    todo!()
}
pub async fn login(State(_state): State<AppState>) -> Response {
    todo!()
}
pub async fn logout(State(_state): State<AppState>) -> Response {
    todo!()
}
