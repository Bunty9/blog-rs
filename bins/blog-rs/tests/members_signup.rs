//! End-to-end HTTP-level signup test.
//!
//! Drives the public member router through `axum::Router::oneshot`:
//!
//!     POST /signup          → row inserted (unconfirmed) + confirm email enqueued
//!     worker::outbox::tick  → file mailer writes the rendered confirm email
//!     GET  /confirm/:token  → row flips to confirmed
//!     POST /unsubscribe/:t  → row flips to unsubscribed
//!
//! The full plan-step E2E (browser-driven) lives in the Playwright suite added
//! in Task 19; this test keeps the same shape inside the test process so we
//! catch route/state regressions without spinning up a browser.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
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

use state::{AppState, SiteConfig};

async fn boot() -> (axum::Router, AppState, std::path::PathBuf) {
    let pool = db::test_support::fresh_pool().await;

    // FK placeholder so enqueue_confirm (post_id=0) satisfies the schema.
    let now = time::OffsetDateTime::now_utc().unix_timestamp();
    sqlx::query(
        "INSERT INTO users (id, email, password_hash, role, created_at)
         VALUES (1, '__placeholder__', '', 'admin', ?)
         ON CONFLICT(id) DO NOTHING",
    )
    .bind(now)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO posts (id, slug, title, status, author_id, updated_at, created_at,
                            body_md, body_html, deleted_at)
         VALUES (0, '__confirm_marker__', '', 'draft', 1, ?, ?, '', '', ?)",
    )
    .bind(now)
    .bind(now)
    .bind(now)
    .execute(&pool)
    .await
    .unwrap();

    let tmp = tempfile::NamedTempFile::new().unwrap();
    let mailbox_path = tmp.path().to_owned();
    // Drop the file handle; the FileTransport reopens with append-create.
    let _ = tmp.close();

    let mailer = Arc::new(mailer::test_file::FileTransport::new(&mailbox_path));
    let st = AppState::new(pool, config::Config::default(), vec![0u8; 32])
        .with_mailer(mailer)
        .with_site(SiteConfig {
            base_url: "http://localhost".into(),
            site_title: "blog-rs".into(),
            admin_from: "blog-rs <noreply@localhost>".into(),
        });
    let app = routes::router(st.clone());
    (app, st, mailbox_path)
}

fn extract_token_after(raw: &str, needle: &str) -> Option<String> {
    let i = raw.find(needle)? + needle.len();
    let rest = &raw[i..];
    let end = rest
        .find(|c: char| {
            c.is_whitespace() || c == '<' || c == '"' || c == ')' || c == ',' || c == '='
        })
        .unwrap_or(rest.len());
    Some(rest[..end].to_string())
}

/// quoted-printable decode the relevant chunks we care about.
/// Lettre emits the rendered HTML body QP-encoded, which means our `/confirm/`
/// URL gets a `=\r\n` soft-wrap injected if it crosses the 76-char boundary
/// and bytes like `=` get `=3D` escaped. This minimal decoder strips both so
/// the substring search and the token extraction work on the original URL.
fn qp_decode(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'=' && i + 2 < bytes.len() {
            // Soft line break: `=\r\n` or `=\n`
            if bytes[i + 1] == b'\r' && bytes[i + 2] == b'\n' {
                i += 3;
                continue;
            }
            if bytes[i + 1] == b'\n' {
                i += 2;
                continue;
            }
            // Hex escape: =XX
            let hi = bytes[i + 1] as char;
            let lo = bytes[i + 2] as char;
            if let (Some(h), Some(l)) = (hi.to_digit(16), lo.to_digit(16)) {
                out.push(((h * 16) + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(b);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[tokio::test]
async fn signup_then_confirm_then_unsubscribe() {
    let (app, st, mailbox_path) = boot().await;

    // 1) POST /signup. No cookie is present, so the CSRF check on the submit
    //    handler falls through (see signup::submit: when no cookie exists,
    //    the form's csrf_token is accepted to support anonymous public posts).
    let body = "email=alice%40example.com&csrf_token=anything";
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/signup")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "/signup should accept");

    // 2) member row exists, unconfirmed.
    let m = db::members::find_by_id(&st.pool, 1).await.unwrap();
    assert_eq!(m.email, "alice@example.com");
    assert!(m.confirmed_at.is_none());

    // 3) Synchronous send already wrote the confirm email to the mailbox.
    //    Draining the outbox once is still a no-op for the same row (it was
    //    marked sent? actually the signup handler only sends synchronously
    //    without status update — the outbox row is still pending). Tick once
    //    to flush so the worker path is exercised too; either ordering should
    //    leave at least one rendered message on disk.
    let _ = worker::outbox::tick(&st, 10, 3).await;

    let raw_bytes = tokio::fs::read(&mailbox_path).await.unwrap();
    let raw = String::from_utf8_lossy(&raw_bytes).into_owned();
    let raw = qp_decode(&raw);
    assert!(
        raw.contains("To: alice@example.com"),
        "to header missing:\n{raw}"
    );
    assert!(raw.contains("Subject:"), "subject header missing");
    assert!(raw.contains("/confirm/"), "confirm URL missing");

    // 4) Extract the confirm token and exchange it for a confirmed row.
    let token = extract_token_after(&raw, "/confirm/").expect("token present in mailbox");
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/confirm/{token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "/confirm should succeed");
    let m = db::members::find_by_id(&st.pool, 1).await.unwrap();
    assert!(m.confirmed_at.is_some(), "row should be confirmed");

    // 5) Issue an unsubscribe token directly (the worker would normally emit it
    //    in a post email) and POST to /unsubscribe.
    let unsub = st.tokens.issue(1, tokens::Purpose::Unsubscribe).unwrap();
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/unsubscribe/{unsub}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "/unsubscribe should succeed");
    let m = db::members::find_by_id(&st.pool, 1).await.unwrap();
    assert!(m.unsubscribed_at.is_some(), "row should be unsubscribed");
}
