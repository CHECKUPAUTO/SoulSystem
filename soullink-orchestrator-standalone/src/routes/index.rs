//! Route: Index / API Documentation

use crate::models::types::ApiIndex;
use axum::Json;
use serde_json::Value;

/// Route GET /
pub async fn route_index() -> Json<Value> {
    let index = ApiIndex::default();
    Json(serde_json::to_value(index).unwrap())
}
