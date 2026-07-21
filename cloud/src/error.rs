use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    BadRequest(String),
    #[error("authentication required")]
    Unauthorized,
    #[error("permission denied")]
    Forbidden,
    #[error("resource not found")]
    NotFound,
    #[error("{0}")]
    Conflict(String),
    #[error("unsupported media type")]
    UnsupportedMediaType,
    #[error("database operation failed")]
    Database(#[from] sqlx::Error),
    #[error("internal service error: {0}")]
    Internal(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, public_message) = match &self {
            Self::BadRequest(message) => (StatusCode::BAD_REQUEST, message.as_str()),
            Self::Unauthorized => (StatusCode::UNAUTHORIZED, "authentication required"),
            Self::Forbidden => (StatusCode::FORBIDDEN, "permission denied"),
            Self::NotFound => (StatusCode::NOT_FOUND, "resource not found"),
            Self::Conflict(message) => (StatusCode::CONFLICT, message.as_str()),
            Self::UnsupportedMediaType => (
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "only UTF-8 Markdown content can be read",
            ),
            Self::Database(_) | Self::Internal(_) => {
                tracing::error!(error = %self, "request failed");
                (StatusCode::INTERNAL_SERVER_ERROR, "internal service error")
            }
        };
        let mut response = (status, Json(json!({ "error": public_message }))).into_response();
        if status == StatusCode::UNAUTHORIZED {
            response.headers_mut().insert(
                axum::http::header::WWW_AUTHENTICATE,
                axum::http::HeaderValue::from_static("Bearer realm=\"cowiki\""),
            );
        }
        response
    }
}

pub type AppResult<T> = Result<T, AppError>;
