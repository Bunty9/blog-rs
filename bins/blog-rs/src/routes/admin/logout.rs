//! POST /admin/logout — destroys the current session and clears both cookies.

use axum::extract::{Extension, State};
use axum::http::{header::SET_COOKIE, HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::post;
use axum::{Json, Router};
use cookie::Cookie;
use serde_json::json;

use crate::error::AppError;
use crate::middleware::auth_required::{self, SessionCtx};
use crate::middleware::csrf;
use crate::state::AppState;

pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/logout", post(handler))
        // Layers apply bottom-up: auth_required runs first (populates SessionCtx),
        // then csrf validates the header against the loaded session.
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            csrf::layer,
        ))
        .layer(axum::middleware::from_fn_with_state(
            state,
            auth_required::layer,
        ))
}

async fn handler(
    State(state): State<AppState>,
    Extension(session): Extension<SessionCtx>,
    headers: HeaderMap,
) -> Result<axum::response::Response, AppError> {
    let token = headers
        .get(axum::http::header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| {
            Cookie::split_parse(s)
                .filter_map(|c| c.ok())
                .find(|c| c.name() == auth::session::SESSION_COOKIE)
                .map(|c| c.value().to_string())
        })
        .ok_or(AppError::Unauthorized)?;

    let _ = session.user_id; // session loaded by middleware, kept for the trace span
    db::sessions::destroy(&state.pool, &token).await?;

    let mut out = HeaderMap::new();
    out.append(
        SET_COOKIE,
        auth::session::expire_cookie(auth::session::SESSION_COOKIE)
            .to_string()
            .parse()
            .unwrap(),
    );
    out.append(
        SET_COOKIE,
        auth::session::expire_cookie(auth::session::CSRF_COOKIE)
            .to_string()
            .parse()
            .unwrap(),
    );
    Ok((StatusCode::OK, out, Json(json!({"status": "logged_out"}))).into_response())
}
