use axum::{
    Json,
    response::{IntoResponse, Response},
};

pub async fn health_check() -> Response {
    Json("ok").into_response()
}

pub async fn create_new_order() -> Response {
    todo!()
}
pub async fn get_order_by_id() -> Response {
    todo!()
}
pub async fn cancel_order() -> Response {
    todo!()
}
pub async fn modify_order() -> Response {
    todo!()
}
