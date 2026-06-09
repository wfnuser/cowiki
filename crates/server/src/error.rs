use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

pub type Result<T> = std::result::Result<T, AppError>;

#[derive(Debug)]
pub enum AppError {
    Internal(String),
    NotFound(String),
    BadRequest(String),
    Unauthorized(String),
    Forbidden(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, msg) = match self {
            AppError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
            AppError::Unauthorized(msg) => (StatusCode::UNAUTHORIZED, msg),
            AppError::Forbidden(msg) => (StatusCode::FORBIDDEN, msg),
        };
        (status, msg).into_response()
    }
}

impl From<sqlx::Error> for AppError {
    fn from(e: sqlx::Error) -> Self {
        AppError::Internal(e.to_string())
    }
}

impl From<git2::Error> for AppError {
    fn from(e: git2::Error) -> Self {
        AppError::Internal(e.to_string())
    }
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;
    use axum::response::IntoResponse;

    fn status_of(err: AppError) -> StatusCode {
        err.into_response().status()
    }

    #[test]
    fn test_internal_returns_500() {
        assert_eq!(
            status_of(AppError::Internal("boom".into())),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[test]
    fn test_not_found_returns_404() {
        assert_eq!(
            status_of(AppError::NotFound("gone".into())),
            StatusCode::NOT_FOUND
        );
    }

    #[test]
    fn test_bad_request_returns_400() {
        assert_eq!(
            status_of(AppError::BadRequest("wrong".into())),
            StatusCode::BAD_REQUEST
        );
    }

    #[test]
    fn test_unauthorized_returns_401() {
        assert_eq!(
            status_of(AppError::Unauthorized("no access".into())),
            StatusCode::UNAUTHORIZED
        );
    }

    #[test]
    fn test_forbidden_returns_403() {
        assert_eq!(
            status_of(AppError::Forbidden("not allowed".into())),
            StatusCode::FORBIDDEN
        );
    }

    #[test]
    fn test_all_error_status_codes_distinct() {
        let errors = vec![
            AppError::Internal("".into()),
            AppError::NotFound("".into()),
            AppError::BadRequest("".into()),
            AppError::Unauthorized("".into()),
            AppError::Forbidden("".into()),
        ];

        let codes: Vec<StatusCode> = errors
            .into_iter()
            .map(|e| e.into_response().status())
            .collect();

        use std::collections::HashSet;
        let unique: HashSet<_> = codes.into_iter().collect();
        assert_eq!(
            unique.len(),
            5,
            "all error types should have distinct status codes"
        );
    }

    #[test]
    fn test_result_type_alias_ok() {
        let ok: Result<String> = Ok("hello".into());
        assert_eq!(ok.unwrap(), "hello");
    }

    #[test]
    fn test_result_type_alias_forbidden_err() {
        let err: Result<String> = Err(AppError::Forbidden("nope".into()));
        assert!(err.is_err());
        match err.unwrap_err() {
            AppError::Forbidden(msg) => assert_eq!(msg, "nope"),
            _ => panic!("expected Forbidden"),
        }
    }
}
