//! Reject mutating requests whose `X-CSRF-Token` header (or form field) does
//! not match the current session's csrf_token.

use axum::extract::{Request, State};
use axum::http::Method;
use axum::middleware::Next;
use axum::response::Response;
use cookie::Cookie;

use crate::error::AppError;
use crate::middleware::auth_required::SessionCtx;
use crate::state::AppState;

pub async fn layer(
    State(_state): State<AppState>,
    req: Request,
    next: Next,
) -> Result<Response, AppError> {
    let mutating = matches!(
        *req.method(),
        Method::POST | Method::PUT | Method::PATCH | Method::DELETE
    );
    if !mutating {
        return Ok(next.run(req).await);
    }

    let session = req
        .extensions()
        .get::<SessionCtx>()
        .ok_or(AppError::Unauthorized)?
        .clone();

    let submitted = req
        .headers()
        .get("x-csrf-token")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .or_else(|| {
            req.headers()
                .get(axum::http::header::COOKIE)
                .and_then(|v| v.to_str().ok())
                .and_then(|s| {
                    Cookie::split_parse(s)
                        .filter_map(|c| c.ok())
                        .find(|c| c.name() == auth::session::CSRF_COOKIE)
                        .map(|c| c.value().to_string())
                })
        })
        .ok_or(AppError::Forbidden)?;

    auth::csrf::validate(&session.csrf_token, &submitted)?;
    Ok(next.run(req).await)
}
