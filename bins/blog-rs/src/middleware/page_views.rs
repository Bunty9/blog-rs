//! Records anonymous, aggregate page views for the public reader surface.
//!
//! Privacy model: cookieless, no IP addresses, no user agents, and no
//! per-visitor identifier of any kind are ever stored. A "view" is a single
//! increment of a `(day, path)` counter (see `db::analytics::record_view`) --
//! there is nothing here that can be traced back to a visitor or linked
//! across requests.
//!
//! A view is recorded only when all of the following hold:
//! - method is GET and the response status is 200
//! - the response content type is `text/html`
//! - the path isn't under an excluded prefix (admin, assets, media, health)
//! - the request isn't an htmx partial (`HX-Request` header present)
//! - the visitor didn't send `DNT: 1` or `Sec-GPC: 1`
//! - the User-Agent doesn't look like a bot/crawler/script
//!
//! The DB write happens after the response is built, in a spawned task, so
//! it never delays the response; failures are logged and never surface to
//! the client. In tests the write is awaited inline instead of spawned, so
//! assertions don't race a background task.

use axum::extract::{Request, State};
use axum::http::{header, HeaderMap, Method, StatusCode};
use axum::middleware::Next;
use axum::response::Response;

use crate::state::AppState;

const EXCLUDED_PREFIXES: &[&str] = &["/admin", "/assets", "/media", "/healthz", "/readyz"];

const BOT_MARKERS: &[&str] = &[
    "bot",
    "crawl",
    "spider",
    "slurp",
    "facebookexternalhit",
    "preview",
    "curl",
    "wget",
    "python",
    "headless",
];

/// Max stored path length. Anything longer is truncated (character-boundary
/// safe) rather than rejected -- this is best-effort analytics, not a
/// validated input.
const MAX_PATH_LEN: usize = 512;

pub async fn layer(State(state): State<AppState>, req: Request, next: Next) -> Response {
    let method = req.method().clone();
    let raw_path = req.uri().path().to_string();
    let req_headers = req.headers().clone();

    let resp = next.run(req).await;

    if should_record(
        &method,
        &raw_path,
        resp.status(),
        &req_headers,
        resp.headers(),
    ) {
        let pool = state.pool.clone();
        let path = normalize_path(&raw_path);
        let referrer = referrer_host(&req_headers, own_host(&state.site.base_url).as_deref());

        let record = async move {
            let today = time::OffsetDateTime::now_utc().date();
            let day = db::analytics::fmt_day(today);
            if let Err(e) =
                db::analytics::record_view(&pool, &day, &path, referrer.as_deref()).await
            {
                tracing::warn!(error = ?e, "failed to record page view");
            }
        };

        // See module docs: awaited inline under test so assertions don't race
        // the write, spawned otherwise so recording never delays a response.
        #[cfg(test)]
        record.await;
        #[cfg(not(test))]
        tokio::spawn(record);
    }

    resp
}

fn should_record(
    method: &Method,
    path: &str,
    status: StatusCode,
    req_headers: &HeaderMap,
    resp_headers: &HeaderMap,
) -> bool {
    if method != Method::GET || status != StatusCode::OK {
        return false;
    }
    if EXCLUDED_PREFIXES.iter().any(|p| path.starts_with(p)) {
        return false;
    }
    if req_headers.contains_key("hx-request") {
        return false;
    }
    if opted_out(req_headers) || is_bot_ua(req_headers) {
        return false;
    }
    resp_headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|ct| ct.starts_with("text/html"))
}

fn opted_out(headers: &HeaderMap) -> bool {
    let is_one = |name: &str| headers.get(name).and_then(|v| v.to_str().ok()) == Some("1");
    is_one("dnt") || is_one("sec-gpc")
}

fn is_bot_ua(headers: &HeaderMap) -> bool {
    let ua = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();
    BOT_MARKERS.iter().any(|m| ua.contains(m))
}

/// Strip the query string (already gone via `Uri::path()`) and any trailing
/// slash except on the root, then cap the length.
fn normalize_path(raw: &str) -> String {
    let trimmed = if raw.len() > 1 {
        raw.trim_end_matches('/')
    } else {
        raw
    };
    let trimmed = if trimmed.is_empty() { "/" } else { trimmed };
    trimmed.chars().take(MAX_PATH_LEN).collect()
}

/// The site's own host (lowercase, `www.` stripped), used to drop
/// self-referrals.
fn own_host(base_url: &str) -> Option<String> {
    url::Url::parse(base_url)
        .ok()
        .and_then(|u| u.host_str().map(normalize_host))
}

fn normalize_host(h: &str) -> String {
    let h = h.to_ascii_lowercase();
    h.strip_prefix("www.").map(str::to_string).unwrap_or(h)
}

fn referrer_host(headers: &HeaderMap, own: Option<&str>) -> Option<String> {
    let raw = headers.get(header::REFERER).and_then(|v| v.to_str().ok())?;
    let host = normalize_host(url::Url::parse(raw).ok()?.host_str()?);
    if own == Some(host.as_str()) {
        None
    } else {
        Some(host)
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn normalize_path_trims_trailing_slash_but_keeps_root() {
        assert_eq!(normalize_path("/foo/"), "/foo");
        assert_eq!(normalize_path("/"), "/");
        assert_eq!(normalize_path("/foo"), "/foo");
    }

    #[test]
    fn normalize_path_caps_length() {
        let long = "/".to_string() + &"a".repeat(1000);
        assert_eq!(normalize_path(&long).chars().count(), MAX_PATH_LEN);
    }

    #[test]
    fn bot_markers_are_case_insensitive() {
        let mut h = HeaderMap::new();
        h.insert(
            header::USER_AGENT,
            "Mozilla/5.0 (compatible; Googlebot/2.1)".parse().unwrap(),
        );
        assert!(is_bot_ua(&h));

        let mut h2 = HeaderMap::new();
        h2.insert(header::USER_AGENT, "curl/8.0".parse().unwrap());
        assert!(is_bot_ua(&h2));

        let mut h3 = HeaderMap::new();
        h3.insert(
            header::USER_AGENT,
            "Mozilla/5.0 (Macintosh)".parse().unwrap(),
        );
        assert!(!is_bot_ua(&h3));
    }

    #[test]
    fn referrer_host_drops_self_referrer() {
        let mut h = HeaderMap::new();
        h.insert(
            header::REFERER,
            "https://www.example.com/post/x".parse().unwrap(),
        );
        assert_eq!(referrer_host(&h, Some("example.com")), None);

        let mut h2 = HeaderMap::new();
        h2.insert(
            header::REFERER,
            "https://news.ycombinator.com/item?id=1".parse().unwrap(),
        );
        assert_eq!(
            referrer_host(&h2, Some("example.com")),
            Some("news.ycombinator.com".to_string())
        );
    }

    #[test]
    fn own_host_strips_www_and_lowercases() {
        assert_eq!(
            own_host("https://WWW.Example.com"),
            Some("example.com".to_string())
        );
    }
}

#[cfg(test)]
mod integration_tests {
    use axum::body::Body;
    use axum::http::{header, Request, StatusCode};
    use tower::ServiceExt;

    use crate::config::Config;
    use crate::state::AppState;

    async fn test_app() -> (axum::Router, AppState) {
        let pool = db::test_support::fresh_pool().await;
        let state = AppState::new(pool, Config::default(), vec![0u8; 32]);
        let app = crate::routes::router(state.clone());
        (app, state)
    }

    /// Seeds a published post owned by a throwaway author row (the `posts`
    /// table FKs to `users`) without touching the admin bootstrap path, so
    /// tests that also seed an admin session still see an empty `users`
    /// table beforehand.
    async fn seed_published_post(pool: &db::SqlitePool, slug: &str) {
        sqlx::query(
            "INSERT OR IGNORE INTO users (id, email, password_hash, role, created_at) \
             VALUES (1, 'a@b', 'x', 'admin', 0)",
        )
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            r#"
            INSERT INTO posts (slug, title, status, author_id, published_at,
                               updated_at, created_at, body_md, body_html,
                               body_html_version, meta_json, assets_json)
            VALUES (?, 'Test Post', 'published', 1, 1700000000, 1700000000,
                    1700000000, '# x', '<p>x</p>', ?, '{}', '[]')
            "#,
        )
        .bind(slug)
        .bind(content::RENDER_VERSION as i64)
        .execute(pool)
        .await
        .unwrap();
    }

    fn today_str() -> String {
        db::analytics::fmt_day(time::OffsetDateTime::now_utc().date())
    }

    async fn total_page_views(pool: &db::SqlitePool) -> i64 {
        sqlx::query_scalar("SELECT COALESCE(SUM(views), 0) FROM page_views_daily")
            .fetch_one(pool)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn get_published_post_records_one_view() {
        let (app, state) = test_app().await;
        seed_published_post(&state.pool, "hello").await;

        let res = app
            .oneshot(
                Request::builder()
                    .uri("/posts/hello")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        let views: i64 =
            sqlx::query_scalar("SELECT views FROM page_views_daily WHERE day = ? AND path = ?")
                .bind(today_str())
                .bind("/posts/hello")
                .fetch_one(&state.pool)
                .await
                .unwrap();
        assert_eq!(views, 1);
    }

    #[tokio::test]
    async fn bot_user_agent_does_not_record() {
        let (app, state) = test_app().await;
        seed_published_post(&state.pool, "hello").await;

        let res = app
            .oneshot(
                Request::builder()
                    .uri("/posts/hello")
                    .header(
                        header::USER_AGENT,
                        "Mozilla/5.0 (compatible; Googlebot/2.1)",
                    )
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(total_page_views(&state.pool).await, 0);
    }

    #[tokio::test]
    async fn dnt_header_does_not_record() {
        let (app, state) = test_app().await;
        seed_published_post(&state.pool, "hello").await;

        let res = app
            .oneshot(
                Request::builder()
                    .uri("/posts/hello")
                    .header("dnt", "1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(total_page_views(&state.pool).await, 0);
    }

    #[tokio::test]
    async fn sec_gpc_header_does_not_record() {
        let (app, state) = test_app().await;
        seed_published_post(&state.pool, "hello").await;

        let res = app
            .oneshot(
                Request::builder()
                    .uri("/posts/hello")
                    .header("sec-gpc", "1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(total_page_views(&state.pool).await, 0);
    }

    #[tokio::test]
    async fn admin_path_does_not_record() {
        let (app, state) = test_app().await;

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
        let cookie = format!("{}={}", auth::session::SESSION_COOKIE, session_token);

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
        assert_eq!(total_page_views(&state.pool).await, 0);
    }

    #[tokio::test]
    async fn not_found_post_does_not_record() {
        let (app, state) = test_app().await;

        let res = app
            .oneshot(
                Request::builder()
                    .uri("/posts/does-not-exist")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
        assert_eq!(total_page_views(&state.pool).await, 0);
    }

    #[tokio::test]
    async fn referrer_recorded_and_self_referrer_dropped() {
        let (app, state) = test_app().await;
        seed_published_post(&state.pool, "hello").await;

        // Self-referrer (matches AppState::new's default SiteConfig base_url,
        // "http://localhost:8080") must not create a referrer row.
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/posts/hello")
                    .header(header::REFERER, "http://localhost:8080/somewhere")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let ref_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM referrers_daily")
            .fetch_one(&state.pool)
            .await
            .unwrap();
        assert_eq!(ref_count, 0, "self-referrer should not be recorded");

        // A cross-site referrer is recorded by host.
        let res2 = app
            .oneshot(
                Request::builder()
                    .uri("/posts/hello")
                    .header(header::REFERER, "https://news.ycombinator.com/item?id=1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res2.status(), StatusCode::OK);

        let host_views: i64 =
            sqlx::query_scalar("SELECT views FROM referrers_daily WHERE day = ? AND host = ?")
                .bind(today_str())
                .bind("news.ycombinator.com")
                .fetch_one(&state.pool)
                .await
                .unwrap();
        assert_eq!(host_views, 1);

        // Both requests still counted as page views.
        assert_eq!(total_page_views(&state.pool).await, 2);
    }
}
