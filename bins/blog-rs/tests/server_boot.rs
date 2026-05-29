// Integration smoke test that boots the same router the binary uses, against
// a fresh in-memory pool. Binaries cannot be linked as libraries, so we mirror
// the binary's module tree here using `#[path = "..."]` includes. Each
// integration test file is its own crate; the paths are relative to this file.

use axum::body::Body;
use axum::http::Request;
use tower::ServiceExt;

#[path = "../src/config.rs"]
mod config;
#[path = "../src/embed.rs"]
mod embed;
#[path = "../src/error.rs"]
mod error;
#[path = "../src/middleware/mod.rs"]
mod middleware;
#[path = "../src/routes/mod.rs"]
mod routes;
#[path = "../src/state.rs"]
mod state;
#[path = "../src/view.rs"]
mod view;

#[tokio::test]
async fn healthz_returns_ok_through_full_stack() {
    let pool = db::test_support::fresh_pool().await;
    let cfg = config::Config::default();
    let st = state::AppState::new(pool, cfg, vec![0u8; 32]);
    let app = routes::router(st);

    let res = app
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), axum::http::StatusCode::OK);
}

#[tokio::test]
async fn readyz_returns_ok_through_full_stack() {
    let pool = db::test_support::fresh_pool().await;
    let cfg = config::Config::default();
    let st = state::AppState::new(pool, cfg, vec![0u8; 32]);
    let app = routes::router(st);

    let res = app
        .oneshot(
            Request::builder()
                .uri("/readyz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), axum::http::StatusCode::OK);
}

#[tokio::test]
async fn correlation_id_echoed() {
    let pool = db::test_support::fresh_pool().await;
    let cfg = config::Config::default();
    let st = state::AppState::new(pool, cfg, vec![0u8; 32]);
    let app = routes::router(st);

    let res = app
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .header("x-request-id", "abc-123")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        res.headers().get("x-request-id").unwrap().to_str().unwrap(),
        "abc-123"
    );
}

#[tokio::test]
async fn missing_correlation_id_minted() {
    let pool = db::test_support::fresh_pool().await;
    let cfg = config::Config::default();
    let st = state::AppState::new(pool, cfg, vec![0u8; 32]);
    let app = routes::router(st);

    let res = app
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let id = res.headers().get("x-request-id").unwrap().to_str().unwrap();
    assert!(!id.is_empty());
    // UUID v4 string is 36 chars
    assert_eq!(id.len(), 36);
}

#[tokio::test]
async fn embedded_asset_served() {
    let pool = db::test_support::fresh_pool().await;
    let cfg = config::Config::default();
    let st = state::AppState::new(pool, cfg, vec![0u8; 32]);
    let app = routes::router(st);

    let res = app
        .oneshot(
            Request::builder()
                .uri("/assets/reset.css")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), axum::http::StatusCode::OK);
    let ct = res.headers().get("content-type").unwrap().to_str().unwrap();
    assert!(ct.starts_with("text/css"));
}

#[tokio::test]
async fn embedded_admin_css_served() {
    let pool = db::test_support::fresh_pool().await;
    let cfg = config::Config::default();
    let st = state::AppState::new(pool, cfg, vec![0u8; 32]);
    let app = routes::router(st);

    let res = app
        .oneshot(
            Request::builder()
                .uri("/assets/admin/admin.css")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), axum::http::StatusCode::OK);
    let ct = res.headers().get("content-type").unwrap().to_str().unwrap();
    assert!(ct.starts_with("text/css"));
}

#[tokio::test]
async fn embedded_admin_js_served() {
    let pool = db::test_support::fresh_pool().await;
    let cfg = config::Config::default();
    let st = state::AppState::new(pool, cfg, vec![0u8; 32]);
    let app = routes::router(st);

    let res = app
        .oneshot(
            Request::builder()
                .uri("/assets/admin/htmx.min.js")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), axum::http::StatusCode::OK);
    let ct = res.headers().get("content-type").unwrap().to_str().unwrap();
    assert!(ct.contains("javascript"));
}
