//! GET /admin/analytics — analytics shell (empty state; no data backend yet).

use askama::Template;
use askama_axum::IntoResponse;
use axum::Extension;

use crate::error::AppError;
use crate::middleware::auth_required::SessionCtx;

#[derive(Template)]
#[template(path = "admin/analytics.html")]
struct AnalyticsTpl {
    csrf: String,
    nav: &'static str,
    page_title: &'static str,
}

pub async fn handler(
    Extension(session): Extension<SessionCtx>,
) -> Result<impl IntoResponse, AppError> {
    Ok(AnalyticsTpl {
        csrf: session.csrf_token,
        nav: "analytics",
        page_title: "Analytics",
    })
}

#[cfg(test)]
mod tests {
    use crate::config::Config;
    use axum::body::{to_bytes, Body};
    use axum::http::{header, Request, StatusCode};
    use db::test_support::fresh_pool;
    use tower::ServiceExt;

    use crate::state::AppState;

    async fn test_app() -> (axum::Router, AppState) {
        let pool = fresh_pool().await;
        let state = AppState::new(pool, Config::default(), vec![0u8; 32]);
        let app = crate::routes::router(state.clone());
        (app, state)
    }

    async fn seed_admin_session(state: &AppState) -> (String, String) {
        let hash = auth::password::hash("hunter2").unwrap();
        db::users::bootstrap_admin(&state.pool, "admin@example.com", &hash)
            .await
            .unwrap();
        let user_id = db::users::find_by_email(&state.pool, "admin@example.com")
            .await
            .unwrap()
            .id;
        let session_token = auth::session::mint_token();
        let csrf = auth::session::mint_token();
        let expires = time::OffsetDateTime::now_utc().unix_timestamp() + 3600;
        db::sessions::create(&state.pool, &session_token, user_id, &csrf, expires)
            .await
            .unwrap();
        (session_token, csrf)
    }

    #[tokio::test]
    async fn analytics_unauth_returns_401() {
        let (app, _state) = test_app().await;
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/admin/analytics")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn analytics_auth_renders_shell() {
        let (app, state) = test_app().await;
        let (sid, _csrf) = seed_admin_session(&state).await;

        let cookie = format!("{}={}", auth::session::SESSION_COOKIE, sid);
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/admin/analytics")
                    .header(header::COOKIE, cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let body = std::str::from_utf8(&bytes).unwrap();
        assert!(body.contains("Analytics"), "title missing: {body}");
        assert!(
            body.contains("analytics module"),
            "empty state text missing: {body}"
        );
    }
}
