//! Loads the active session from the `__Host-sid` cookie and attaches a
//! `SessionCtx` extension. 401s if missing/expired.

use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::Response;
use cookie::Cookie;

use crate::error::AppError;
use crate::state::AppState;

#[derive(Debug, Clone)]
pub struct SessionCtx {
    pub user_id: i64,
    pub csrf_token: String,
}

pub async fn layer(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Result<Response, AppError> {
    let cookies = req
        .headers()
        .get(axum::http::header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let token = Cookie::split_parse(cookies)
        .filter_map(|c| c.ok())
        .find(|c| c.name() == auth::session::SESSION_COOKIE)
        .map(|c| c.value().to_string())
        .ok_or(AppError::Unauthorized)?;

    let s = db::sessions::find_active(&state.pool, &token)
        .await
        .map_err(|_| AppError::Unauthorized)?;

    req.extensions_mut().insert(SessionCtx {
        user_id: s.user_id,
        csrf_token: s.csrf_token,
    });
    Ok(next.run(req).await)
}
