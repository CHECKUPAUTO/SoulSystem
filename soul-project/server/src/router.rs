use axum::{
    routing::post,
    Router,
};
use crate::AppState;
use crate::handlers;

pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route("/api/auth/register", post(handlers::auth::register))
        .route("/api/auth/login", post(handlers::auth::login))
        .route("/api/conversations/:id/messages", post(handlers::messages::send_message))
        .with_state(state)
}
