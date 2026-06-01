//! GET /admin — dashboard with counts + recent activity table.

use askama::Template;
use askama_axum::IntoResponse;
use axum::extract::State;
use axum::Extension;
use db::{members, posts};

use crate::error::AppError;
use crate::middleware::auth_required::SessionCtx;
use crate::state::AppState;

#[derive(Template)]
#[template(path = "admin/dashboard.html")]
struct DashboardTpl {
    csrf: String,
    nav: &'static str,
    page_title: &'static str,
    drafts: i64,
    scheduled: i64,
    published: i64,
    members_total: i64,
    members_confirmed: i64,
    recent: Vec<posts::AdminPostRow>,
}

pub async fn handler(
    State(state): State<AppState>,
    Extension(session): Extension<SessionCtx>,
) -> Result<impl IntoResponse, AppError> {
    let (drafts, scheduled, published) = posts::dashboard_counts(&state.pool).await?;
    let (members_total, members_confirmed, _) = members::count_all(&state.pool).await?;
    let recent = posts::list_admin(&state.pool, posts::PostStatusFilter::All, None, 10).await?;

    Ok(DashboardTpl {
        csrf: session.csrf_token,
        nav: "dashboard",
        page_title: "Dashboard",
        drafts,
        scheduled,
        published,
        members_total,
        members_confirmed,
        recent,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use axum::body::{to_bytes, Body};
    use axum::http::{header, Request, StatusCode};
    use db::test_support::fresh_pool;
    use tower::ServiceExt;

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
    async fn dashboard_unauth_returns_401() {
        let (app, _state) = test_app().await;
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/admin")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn dashboard_auth_renders_stat_grid() {
        let (app, state) = test_app().await;
        let (sid, _csrf) = seed_admin_session(&state).await;

        let cookie = format!("{}={}", auth::session::SESSION_COOKIE, sid);
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/admin")
                    .header(header::COOKIE, cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let body = std::str::from_utf8(&bytes).unwrap();
        assert!(body.contains("Dashboard"), "title missing: {body}");
        assert!(body.contains("Drafts"), "drafts label missing");
        assert!(body.contains("Scheduled"), "scheduled label missing");
        assert!(body.contains("Published"), "published label missing");
        assert!(body.contains("Confirmed members"), "members label missing");
    }

    #[tokio::test]
    async fn dashboard_lists_recent_posts() {
        let (app, state) = test_app().await;
        let (sid, _csrf) = seed_admin_session(&state).await;

        sqlx::query(
            r#"
            INSERT INTO posts (slug, title, status, author_id, published_at,
                               updated_at, created_at, excerpt, reading_minutes,
                               body_md, body_html, meta_json, assets_json)
            VALUES ('hello-world', 'Hello Recent', 'published', 1, 100, 100, 100,
                    null, 1, '# x', '<h1>x</h1>', '{}', '[]')
            "#,
        )
        .execute(&state.pool)
        .await
        .unwrap();

        let cookie = format!("{}={}", auth::session::SESSION_COOKIE, sid);
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/admin")
                    .header(header::COOKIE, cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let body = std::str::from_utf8(&bytes).unwrap();
        assert!(body.contains("Hello Recent"), "recent post title missing");
        assert!(body.contains("hello-world"), "recent post slug missing");
    }
}
