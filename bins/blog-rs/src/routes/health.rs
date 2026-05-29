//! /healthz — liveness (always 200 once we're past start-up).
//! /readyz  — readiness: DB ping + outbox-worker heartbeat staleness check.

use std::time::Duration;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use serde_json::json;

use crate::state::AppState;

/// Max heartbeat age before /readyz reports `not_ready`. Chosen as
/// 2× the default outbox poll interval (5s) with headroom for slow ticks.
const HEARTBEAT_STALE_AFTER: Duration = Duration::from_secs(60);
/// Warm-up window: if no heartbeat has landed yet but the process is
/// younger than this, allow /readyz to pass. Worker spawn ordering means
/// the first tick can lag the first HTTP request by a second or two.
const WORKER_WARMUP: Duration = Duration::from_secs(30);

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
}

async fn healthz() -> impl IntoResponse {
    (StatusCode::OK, Json(json!({"status": "ok"})))
}

async fn readyz(State(state): State<AppState>) -> impl IntoResponse {
    if let Err(e) = sqlx::query_scalar::<_, i64>("SELECT 1")
        .fetch_one(&state.pool)
        .await
    {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"status": "not_ready", "error": e.to_string()})),
        )
            .into_response();
    }

    let hb = *state.worker_heartbeat.lock().await;
    match hb {
        Some(t) if t.elapsed() <= HEARTBEAT_STALE_AFTER => {
            (StatusCode::OK, Json(json!({"status": "ready"}))).into_response()
        }
        Some(t) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({
                "status": "not_ready",
                "error": "worker heartbeat stale",
                "age_secs": t.elapsed().as_secs(),
            })),
        )
            .into_response(),
        None => {
            if state.started_at.elapsed() <= WORKER_WARMUP {
                (
                    StatusCode::OK,
                    Json(json!({"status": "ready", "worker": "warming_up"})),
                )
                    .into_response()
            } else {
                (
                    StatusCode::SERVICE_UNAVAILABLE,
                    Json(json!({"status": "not_ready", "error": "worker never ticked"})),
                )
                    .into_response()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use db::test_support::fresh_pool;
    use tower::ServiceExt;

    async fn state() -> AppState {
        let pool = fresh_pool().await;
        let cfg = crate::config::Config::default();
        AppState::new(pool, cfg, vec![0u8; 32])
    }

    #[tokio::test]
    async fn healthz_ok() {
        let app = router().with_state(state().await);
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/healthz")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn readyz_ok_with_live_pool() {
        // No heartbeat yet, but state was just constructed so warm-up grace
        // applies and /readyz returns 200.
        let app = router().with_state(state().await);
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/readyz")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn readyz_not_ready_on_stale_heartbeat() {
        let mut s = state().await;
        // Force an obviously stale heartbeat by stamping it well past the
        // staleness threshold. Subtract a bit more than HEARTBEAT_STALE_AFTER
        // so the elapsed check trips even on slow CI clocks.
        let stale = std::time::Instant::now() - (HEARTBEAT_STALE_AFTER + Duration::from_secs(5));
        {
            let mut hb = s.worker_heartbeat.lock().await;
            *hb = Some(stale);
        }
        // Also age `started_at` so we're definitely past warm-up; this is
        // belt-and-braces — the explicit Some() heartbeat path already
        // bypasses the warm-up window.
        s.started_at = std::time::Instant::now() - (WORKER_WARMUP + Duration::from_secs(5));

        let app = router().with_state(s);
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/readyz")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::SERVICE_UNAVAILABLE);
    }
}
