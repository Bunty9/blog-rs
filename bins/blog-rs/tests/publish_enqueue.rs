//! HTTP-level integration test for `POST /admin/posts/:id/publish`.
//!
//! `db::posts::publish` already covers the SQL fan-out in unit tests
//! (`crates/db/src/posts.rs::publish_fans_out_outbox_to_confirmed_members`).
//! This test exercises the route end-to-end: it logs in via the admin login
//! endpoint, replays the issued session + CSRF cookies on the publish POST,
//! and then asserts the outbox rows landed only for confirmed-and-subscribed
//! members. The test is the protection against a future refactor silently
//! dropping the call to `posts::publish` in the handler.

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use tower::ServiceExt;

#[path = "../src/config.rs"]
mod config;
#[path = "../src/embed.rs"]
mod embed;
#[path = "../src/error.rs"]
mod error;
#[path = "../src/mailer/mod.rs"]
mod mailer;
#[path = "../src/middleware/mod.rs"]
mod middleware;
#[path = "../src/routes/mod.rs"]
mod routes;
#[path = "../src/state.rs"]
mod state;
#[path = "../src/templates.rs"]
mod templates;
#[path = "../src/tokens.rs"]
mod tokens;
#[path = "../src/view.rs"]
mod view;
#[path = "../src/worker/mod.rs"]
mod worker;

use state::AppState;

async fn boot() -> (axum::Router, AppState) {
    let pool = db::test_support::fresh_pool().await;
    let hash = auth::password::hash("hunter2").unwrap();
    db::users::bootstrap_admin(&pool, "admin@example.com", &hash)
        .await
        .unwrap();
    let st = AppState::new(pool, config::Config::default(), vec![0u8; 32]);
    let app = routes::router(st.clone());
    (app, st)
}

/// Log in and pull back the session + csrf cookie pair.
async fn login(app: &axum::Router) -> (String, String) {
    let body = serde_json::to_vec(&serde_json::json!({
        "email": "admin@example.com",
        "password": "hunter2"
    }))
    .unwrap();
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/admin/login")
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK, "login should succeed");

    let mut session_cookie = String::new();
    let mut csrf_cookie = String::new();
    for v in res.headers().get_all(header::SET_COOKIE).iter() {
        let s = v.to_str().unwrap();
        let first = s.split(';').next().unwrap_or("");
        if first.starts_with(auth::session::SESSION_COOKIE) {
            session_cookie = first.to_string();
        } else if first.starts_with(auth::session::CSRF_COOKIE) {
            csrf_cookie = first.to_string();
        }
    }
    assert!(!session_cookie.is_empty(), "session cookie missing");
    assert!(!csrf_cookie.is_empty(), "csrf cookie missing");
    (session_cookie, csrf_cookie)
}

/// Seed a published-eligible post (status='draft'), an author, and three
/// members in three states. Returns the post id.
async fn seed_post_and_members(st: &AppState) -> i64 {
    let now = time::OffsetDateTime::now_utc().unix_timestamp();
    let author_id: i64 = sqlx::query_scalar("SELECT id FROM users LIMIT 1")
        .fetch_one(&st.pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO posts (slug, title, status, author_id, updated_at, created_at,
                            body_md, body_html)
         VALUES ('hello', 'Hello', 'draft', ?, ?, ?, '# hi', '<h1>hi</h1>')",
    )
    .bind(author_id)
    .bind(now)
    .bind(now)
    .execute(&st.pool)
    .await
    .unwrap();
    let post_id: i64 = sqlx::query_scalar("SELECT id FROM posts WHERE slug='hello'")
        .fetch_one(&st.pool)
        .await
        .unwrap();

    db::members::insert_fixture(&st.pool, "confirmed@example.com", Some(now), None)
        .await
        .unwrap();
    db::members::insert_fixture(&st.pool, "pending@example.com", None, None)
        .await
        .unwrap();
    db::members::insert_fixture(
        &st.pool,
        "unsubscribed@example.com",
        Some(now),
        Some(now),
    )
    .await
    .unwrap();

    post_id
}

#[tokio::test]
async fn publish_enqueues_for_confirmed_members_only() {
    let (app, st) = boot().await;
    let post_id = seed_post_and_members(&st).await;
    let (session_c, csrf_c) = login(&app).await;

    // Extract the raw csrf value (after `XSRF-TOKEN=`) for the header.
    let csrf_value = csrf_c
        .splitn(2, '=')
        .nth(1)
        .unwrap_or_default()
        .to_string();

    let res = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/admin/posts/{post_id}/publish"))
                .header(header::COOKIE, format!("{session_c}; {csrf_c}"))
                .header("x-csrf-token", csrf_value)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK, "publish should succeed");

    // The post is now published.
    let status: String = sqlx::query_scalar("SELECT status FROM posts WHERE id = ?")
        .bind(post_id)
        .fetch_one(&st.pool)
        .await
        .unwrap();
    assert_eq!(status, "published");

    // Exactly one outbox row for the post — the confirmed, not-unsubscribed
    // member. Pending and unsubscribed members are skipped.
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM newsletter_outbox WHERE post_id = ? AND status = 'pending'",
    )
    .bind(post_id)
    .fetch_one(&st.pool)
    .await
    .unwrap();
    assert_eq!(count, 1, "only confirmed-and-subscribed member should be enqueued");

    let recipient: String = sqlx::query_scalar(
        "SELECT m.email FROM newsletter_outbox o
         JOIN members m ON m.id = o.member_id
         WHERE o.post_id = ?",
    )
    .bind(post_id)
    .fetch_one(&st.pool)
    .await
    .unwrap();
    assert_eq!(recipient, "confirmed@example.com");
}

#[tokio::test]
async fn republishing_is_idempotent() {
    let (app, st) = boot().await;
    let post_id = seed_post_and_members(&st).await;
    let (session_c, csrf_c) = login(&app).await;
    let csrf_value = csrf_c
        .splitn(2, '=')
        .nth(1)
        .unwrap_or_default()
        .to_string();

    let send = || {
        let req = Request::builder()
            .method("POST")
            .uri(format!("/admin/posts/{post_id}/publish"))
            .header(header::COOKIE, format!("{session_c}; {csrf_c}"))
            .header("x-csrf-token", &csrf_value)
            .body(Body::empty())
            .unwrap();
        app.clone().oneshot(req)
    };

    let r1 = send().await.unwrap();
    assert_eq!(r1.status(), StatusCode::OK);
    let r2 = send().await.unwrap();
    assert_eq!(r2.status(), StatusCode::OK);

    // Still exactly one row — UNIQUE(post_id, member_id) deduplicates.
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM newsletter_outbox WHERE post_id = ?",
    )
    .bind(post_id)
    .fetch_one(&st.pool)
    .await
    .unwrap();
    assert_eq!(count, 1);
}
