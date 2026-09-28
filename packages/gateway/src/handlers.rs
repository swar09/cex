use axum::{
    Json,
    extract::State,
    response::{IntoResponse, Response},
};
use domain::NewOrder;

use crate::{
    AppState,
    types::{NewOrderReq, ext_order_id_genrator},
};
pub async fn health_check() -> Response {
    Json("ok").into_response()
}

pub async fn create_new_order(new_order_req: NewOrderReq, State(_state): State<AppState>) -> Response {
    // validate jwt in the middleware
    // validate user account permissions
    // use helper fuctions
    let new_id = ext_order_id_genrator();
    let _new_order = NewOrder {
        order_id: new_id,
        order_type: new_order_req.order_type,
        asset_id: new_order_req.asset_id,
        user_id: new_order_req.user_id,
        price: new_order_req.price,
        quantity: new_order_req.quantity,
        side: new_order_req.side,
    };
    // let result = state.cmd.add_new_order(new_order)
    // if result.is_err() ,
    // cache.expire(new_order , some_ttl) order status is changed to
    // expired/rejected then order pushed in cache then  return
    // Error.into_response()

    // if result.is_ok()
    // order inserted in orderbook return 200 ok
    // cache.push(new_order , some_ttl)

    // order matched or rejected or stays in orderbook forever is not part of the
    // rest api if order rejcted or expired another api will send resp to client

    todo!()
}
pub async fn get_order_by_id(State(_state): State<AppState>) -> Response {
    // get order from cache first if not found return 404 or forward to node backend
    // this is not for historical data
    // this api is only for order live in orderbook
    // historical orders are in node backend
    todo!()
}
pub async fn cancel_order(State(_state): State<AppState>) -> Response {
    // send command to exchange
    // state.cmd.cancel_order()
    // if send sucessfully return 200
    // as per my info orderbook will defnetly cancel that order
    // unless it was matched before cancel req arrived at exchange
    // cache.order_cacnelled();
    todo!()
}
pub async fn modify_order(State(_state): State<AppState>) -> Response {
    // send command to exchange
    // state.cmd.modify_order()
    // if send sucessfully return 200
    // as per my info orderbook will defnetly modify that order
    // unless it was matched before modify req arrived at exchange
    // cache.order_modified();
    todo!()
}
pub async fn login(State(_state): State<AppState>) -> Response {
    todo!()
}
pub async fn logout(State(_state): State<AppState>) -> Response {
    todo!()
}
