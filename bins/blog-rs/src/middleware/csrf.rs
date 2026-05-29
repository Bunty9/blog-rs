//! Reject mutating requests whose `X-CSRF-Token` header does not match the
//! current session's csrf_token.
//!
//! Double-submit cookie pattern: the client reads the non-HttpOnly CSRF cookie
//! and echoes its value back in the `X-CSRF-Token` header (HTMX) or as a form
//! field copied into the header (manual fetch). The server NEVER reads the
//! cookie itself for validation, because a browser auto-attaches the cookie
//! on cross-origin POSTs and that would defeat the double-submit guarantee.
//! The second channel (the header) is what proves the request originated from
//! same-origin JavaScript that could read the cookie.

use axum::extract::{Request, State};
use axum::http::Method;
use axum::middleware::Next;
use axum::response::Response;

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

    // Header-only. No cookie fallback: the cookie is auto-sent by the browser
    // and reading it server-side collapses the double-submit pattern into a
    // single-channel check that an attacker on another origin would pass for
    // free.
    let submitted = req
        .headers()
        .get("x-csrf-token")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .ok_or(AppError::Forbidden)?;

    auth::csrf::validate(&session.csrf_token, &submitted)?;
    Ok(next.run(req).await)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use axum::body::Body;
    use axum::http::{header, Request, StatusCode};
    use axum::routing::post;
    use axum::Router;
    use tower::ServiceExt;

    async fn boot() -> (Router, AppState, String, String) {
        let pool = db::test_support::fresh_pool().await;
        let hash = auth::password::hash("hunter2").unwrap();
        db::users::bootstrap_admin(&pool, "admin@example.com", &hash)
            .await
            .unwrap();
        let st = AppState::new(pool, Config::default(), vec![0u8; 32]);

        // Mint a real session via the DB layer so auth_required can find it.
        let session_token = auth::session::mint_token();
        let csrf_value = auth::session::mint_token();
        let user_id: i64 = sqlx::query_scalar("SELECT id FROM users LIMIT 1")
            .fetch_one(&st.pool)
            .await
            .unwrap();
        let expires =
            time::OffsetDateTime::now_utc().unix_timestamp() + st.config.session_lifetime_seconds;
        db::sessions::create(&st.pool, &session_token, user_id, &csrf_value, expires)
            .await
            .unwrap();

        let app = Router::new()
            .route("/probe", post(|| async { "ok" }))
            .layer(axum::middleware::from_fn_with_state(st.clone(), layer))
            .layer(axum::middleware::from_fn_with_state(
                st.clone(),
                crate::middleware::auth_required::layer,
            ))
            .with_state(st.clone());
        (app, st, session_token, csrf_value)
    }

    fn cookie_header(session_token: &str, csrf_value: &str) -> String {
        format!(
            "{}={}; {}={}",
            auth::session::SESSION_COOKIE,
            session_token,
            auth::session::CSRF_COOKIE,
            csrf_value,
        )
    }

    #[tokio::test]
    async fn post_without_csrf_header_but_with_cookie_is_rejected() {
        // Regression: previously the middleware fell back to reading the
        // CSRF cookie when the header was missing. The browser auto-sends
        // the cookie on cross-origin form POSTs, so that fallback defeated
        // the double-submit pattern. Now: missing header => 403, even when
        // the cookie is present.
        let (app, _st, session_token, csrf_value) = boot().await;

        let res = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/probe")
                    .header(header::COOKIE, cookie_header(&session_token, &csrf_value))
                    // No X-CSRF-Token header.
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn post_with_matching_csrf_header_is_accepted() {
        let (app, _st, session_token, csrf_value) = boot().await;

        let res = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/probe")
                    .header(header::COOKIE, cookie_header(&session_token, &csrf_value))
                    .header("x-csrf-token", &csrf_value)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }
}
