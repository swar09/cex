use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use domain::NewOrder;

use crate::{
    AppState,
    exchange::ext_order_id_generator,
    types::{CancelOrderReq, GetOrderQuery, ModifyOrderReq, NewOrderReq},
};

pub const ORDER_DEFAULT_TTL_SEC: u64 = 86400;
pub const REJECTED_ORDER_TTL_SEC: u64 = 60;

pub async fn health_check() -> Response {
    Json("ok").into_response()
}

pub async fn create_new_order(
    State(state): State<AppState>,
    Json(new_order_req): Json<NewOrderReq>,
) -> Response {
    let new_id = ext_order_id_generator();
    let new_order = NewOrder {
        order_id: new_id,
        order_type: new_order_req.order_type,
        asset_id: new_order_req.asset_id,
        user_id: new_order_req.user_id,
        price: new_order_req.price,
        quantity: new_order_req.quantity,
        side: new_order_req.side,
    };

    let result = state.cmd.add_new_order(new_order_req.symbol, new_order.clone());

    if let Err(e) = result {
        let _ = state.cache.push_new_order(&new_order, REJECTED_ORDER_TTL_SEC).await;
        return (StatusCode::SERVICE_UNAVAILABLE, Json(format!("Order rejected: {e}"))).into_response();
    }

    let _ = state.cache.push_new_order(&new_order, ORDER_DEFAULT_TTL_SEC).await;
    (StatusCode::OK, Json(new_order)).into_response()
}

pub async fn get_order_by_id(
    State(state): State<AppState>,
    Query(query): Query<GetOrderQuery>,
) -> Response {
    match state.cache.get_order(query.order_id).await {
        Ok(Some(order)) => (StatusCode::OK, Json(order)).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub async fn cancel_order(
    State(state): State<AppState>,
    Json(req): Json<CancelOrderReq>,
) -> Response {
    let result = state.cmd.cancel_order(req.symbol, req.order_id);
    if let Err(e) = result {
        return (StatusCode::SERVICE_UNAVAILABLE, Json(format!("Cancel failed: {e}"))).into_response();
    }

    let _ = state.cache.order_cancelled(req.order_id).await;
    StatusCode::OK.into_response()
}

pub async fn modify_order(
    State(state): State<AppState>,
    Json(req): Json<ModifyOrderReq>,
) -> Response {
    let result = state.cmd.modify_order(req.symbol, req.modify);
    if let Err(e) = result {
        return (StatusCode::SERVICE_UNAVAILABLE, Json(format!("Modify failed: {e}"))).into_response();
    }

    let _ = state.cache.order_modified(&req.modify).await;
    StatusCode::OK.into_response()
}

pub async fn login(State(_state): State<AppState>) -> Response {
    (StatusCode::OK, Json("login")).into_response()
}

pub async fn logout(State(_state): State<AppState>) -> Response {
    StatusCode::OK.into_response()
}
