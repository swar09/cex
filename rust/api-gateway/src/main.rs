use axum::{
    Json, Router,
    response::{IntoResponse, Response},
    routing::get,
};
use tokio::net::TcpListener;

#[derive(Clone, Debug)]
pub struct AppState {}
#[tokio::main]
async fn main() {
    let state = AppState {};
    let addr = "localhost:300";
    let app = Router::new().route("/healthz", get(health)).with_state(state);
    let listener = TcpListener::bind(addr).await.expect("Failed to init listener");
    axum::serve(listener, app).await.expect("failed to start server");
}
pub async fn health() -> Response {
    Json(String::from("OK")).into_response()
}

