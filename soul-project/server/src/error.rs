use axum::{http::StatusCode, response::{IntoResponse, Response}, Json};
use serde_json::json;
use thiserror::Error;
#[derive(Error, Debug)]
pub enum AppError {
    #[error("Unauthorized")] Unauthorized,
    #[error("Forbidden")] Forbidden,
    #[error("Not Found")] NotFound,
    #[error("Internal Server Error: {0}")] Internal(String),
    #[error("Bad Request: {0}")] BadRequest(String),
    #[error("Conflict: {0}")] Conflict(String),
    #[error("File Too Large")] FileTooLarge,
    #[error("Unsupported File Type")] UnsupportedFileType,
    #[error("Knowledge Budget Exceeded")] KnowledgeBudgetExceeded { current: i64, new: i64, max: i64 },
    #[error("Context Overflow")] ContextOverflow,
}
impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, self.to_string()),
            AppError::Forbidden => (StatusCode::FORBIDDEN, self.to_string()),
            AppError::NotFound => (StatusCode::NOT_FOUND, self.to_string()),
            AppError::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()),
            AppError::BadRequest(_) => (StatusCode::BAD_REQUEST, self.to_string()),
            AppError::Conflict(_) => (StatusCode::CONFLICT, self.to_string()),
            AppError::FileTooLarge => (StatusCode::PAYLOAD_TOO_LARGE, self.to_string()),
            AppError::UnsupportedFileType => (StatusCode::UNSUPPORTED_MEDIA_TYPE, self.to_string()),
            AppError::KnowledgeBudgetExceeded { .. } => (StatusCode::BAD_REQUEST, self.to_string()),
            AppError::ContextOverflow => (StatusCode::BAD_REQUEST, self.to_string()),
        };
        (status, Json(json!({"error": error_message}))).into_response()
    }
}
impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self { match err { sqlx::Error::RowNotFound => AppError::NotFound, _ => AppError::Internal(err.to_string()) } }
}
impl From<anyhow::Error> for AppError {
    fn from(err: anyhow::Error) -> Self { AppError::Internal(err.to_string()) }
}
