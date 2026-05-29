use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;
use thiserror::Error;

// Several variants are constructed by routes that land in Plan 1c+.
#[allow(dead_code)]
#[derive(Debug, Error)]
pub enum AppError {
    #[error("db: {0}")]
    Db(#[from] db::DbError),

    #[error("auth: {0}")]
    Auth(#[from] auth::AuthError),

    #[error("bad request: {0}")]
    BadRequest(String),

    #[error("unauthorized")]
    Unauthorized,

    #[error("forbidden")]
    Forbidden,

    #[error("not found")]
    NotFound,

    #[error("internal: {0}")]
    Internal(String),
}

impl AppError {
    fn status(&self) -> StatusCode {
        match self {
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::Forbidden => StatusCode::FORBIDDEN,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::Db(db::DbError::NotFound) => StatusCode::NOT_FOUND,
            Self::Db(db::DbError::Conflict(_)) => StatusCode::CONFLICT,
            Self::Db(_) | Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Auth(auth::AuthError::BadPassword)
            | Self::Auth(auth::AuthError::CsrfMismatch)
            | Self::Auth(auth::AuthError::TokenSignature)
            | Self::Auth(auth::AuthError::TokenExpired) => StatusCode::UNAUTHORIZED,
            Self::Auth(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.status();
        let body = Json(json!({
            "error": self.to_string(),
            "status": status.as_u16(),
        }));
        tracing::warn!(error = %self, status = %status, "request failed");
        (status, body).into_response()
    }
}
