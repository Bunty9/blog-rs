//! GET /admin/analytics — privacy-friendly page view analytics: totals, a
//! daily bar chart, top pages, and top referrers over a selectable window.
//! Backed by `db::analytics` (aggregate day/path and day/host counters
//! only — no cookies, IPs, user agents, or per-visitor identifiers).

use askama::Template;
use askama_axum::IntoResponse;
use axum::extract::{Query, State};
use axum::Extension;
use db::analytics::{self, HostViews, PathViews};
use serde::Deserialize;

use crate::error::AppError;
use crate::middleware::auth_required::SessionCtx;
use crate::state::AppState;

const VALID_WINDOWS: [i64; 3] = [7, 30, 90];
const TOP_N: i64 = 10;

#[derive(Deserialize, Default)]
pub struct AnalyticsQuery {
    #[serde(default)]
    pub days: Option<i64>,
}

/// One bar in the daily chart, pre-computed so the template stays dumb: a
/// height percentage relative to the busiest day in the window.
pub struct DayBar {
    pub day: String,
    pub views: i64,
    pub pct: u32,
}

#[derive(Template)]
#[template(path = "admin/analytics.html")]
struct AnalyticsTpl {
    csrf: String,
    nav: &'static str,
    page_title: &'static str,
    days: i64,
    total_views: i64,
    views_today: i64,
    has_data: bool,
    bars: Vec<DayBar>,
    top_paths: Vec<PathViews>,
    top_referrers: Vec<HostViews>,
}

pub async fn handler(
    State(state): State<AppState>,
    Extension(session): Extension<SessionCtx>,
    Query(q): Query<AnalyticsQuery>,
) -> Result<impl IntoResponse, AppError> {
    let days = q.days.filter(|d| VALID_WINDOWS.contains(d)).unwrap_or(30);
    let today = time::OffsetDateTime::now_utc().date();

    let totals = analytics::totals_by_day(&state.pool, today, days).await?;
    let total_views: i64 = totals.iter().map(|t| t.views).sum();
    let views_today = totals.last().map(|t| t.views).unwrap_or(0);
    let top_paths = analytics::top_paths(&state.pool, today, days, TOP_N).await?;
    let top_referrers = analytics::top_referrers(&state.pool, today, days, TOP_N).await?;

    let max_views = totals.iter().map(|t| t.views).max().unwrap_or(0).max(1);
    let bars = totals
        .into_iter()
        .map(|t| DayBar {
            pct: ((t.views as f64 / max_views as f64) * 100.0).round() as u32,
            day: t.day,
            views: t.views,
        })
        .collect();

    Ok(AnalyticsTpl {
        csrf: session.csrf_token,
        nav: "analytics",
        page_title: "Analytics",
        days,
        total_views,
        views_today,
        has_data: total_views > 0,
        bars,
        top_paths,
        top_referrers,
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
    async fn analytics_auth_renders_empty_state_with_no_data() {
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
            body.contains("No page views recorded"),
            "empty state text missing: {body}"
        );
    }

    #[tokio::test]
    async fn analytics_with_data_renders_totals_and_top_lists() {
        let (app, state) = test_app().await;
        let (sid, _csrf) = seed_admin_session(&state).await;

        db::analytics::record_view(&state.pool, "2026-09-25", "/posts/hello", Some("lobste.rs"))
            .await
            .unwrap();
        db::analytics::record_view(&state.pool, "2026-09-25", "/posts/hello", Some("lobste.rs"))
            .await
            .unwrap();

        let cookie = format!("{}={}", auth::session::SESSION_COOKIE, sid);
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/admin/analytics?days=30")
                    .header(header::COOKIE, cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let body = std::str::from_utf8(&bytes).unwrap();
        assert!(body.contains("/posts/hello"), "top page missing: {body}");
        assert!(body.contains("lobste.rs"), "top referrer missing: {body}");
    }

    #[tokio::test]
    async fn analytics_rejects_invalid_days_by_falling_back_to_default() {
        let (app, state) = test_app().await;
        let (sid, _csrf) = seed_admin_session(&state).await;

        let cookie = format!("{}={}", auth::session::SESSION_COOKIE, sid);
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/admin/analytics?days=999")
                    .header(header::COOKIE, cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }
}
