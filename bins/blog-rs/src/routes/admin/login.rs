//! GET /admin/login  — HTML login page (standalone card, not behind auth).
//! POST /admin/login — validate credentials, mint session, set cookies (JSON).

use askama::Template;
use askama_axum::IntoResponse;
use axum::extract::{ConnectInfo, State};
use axum::http::{header::SET_COOKIE, HeaderMap, HeaderValue, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use time::{Duration, OffsetDateTime};

use crate::error::AppError;
use crate::rate_limit;
use crate::state::AppState;

// Argon2id hash of the static string "blog-rs-dummy-password" using the
// project parameters. Used to keep the unknown-user code path constant-time
// against the known-user-wrong-password path so login is not a timing oracle.
const DUMMY_HASH: &str = "$argon2id$v=19$m=65536,t=3,p=4$ZHVtbXlzYWx0ZHVtbXlzYWx0$8VV0qpsiBYBb3JoQwlGKqV3v9wmW7XAYqlh4RABh2EE";

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/login", get(form))
        .route("/login", post(submit))
}

#[derive(Debug, Deserialize)]
pub struct LoginForm {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
struct LoginOk {
    user_id: i64,
    csrf_token: String,
}

#[derive(Template)]
#[template(path = "admin/login.html")]
struct LoginTpl {
    error: Option<String>,
}

async fn form() -> impl IntoResponse {
    LoginTpl { error: None }
}

async fn submit(
    State(state): State<AppState>,
    connect_info: Option<ConnectInfo<SocketAddr>>,
    headers: HeaderMap,
    Json(form): Json<LoginForm>,
) -> Result<axum::response::Response, AppError> {
    let ip = rate_limit::client_ip(&headers, connect_info.map(|ci| ci.0));
    let ip_key = format!("login-ip:{ip}");
    let email_key = format!("login-email:{}", form.email.trim().to_lowercase());

    // Count both failed and successful attempts against IP and email; either
    // limiter tripping is a reject. Headers are spoofable off-proxy, hence
    // the email key on top of the IP key.
    let ip_ok = state.login_limiter.check(&ip_key);
    let email_ok = state.login_limiter.check(&email_key);
    if !ip_ok || !email_ok {
        tracing::warn!(email = %form.email, ip = %ip, "login rate limited");
        return Ok(too_many_requests(state.login_limiter.window_secs()));
    }

    let user = match db::users::find_by_email(&state.pool, &form.email).await {
        Ok(u) => u,
        Err(_) => {
            // Spend an Argon2 verification on a dummy hash so the unknown-user
            // path has the same wall-clock cost as the wrong-password path.
            // Result is discarded; we always return Unauthorized here.
            let _ = auth::password::verify(&form.password, DUMMY_HASH);
            return Err(AppError::Unauthorized);
        }
    };
    auth::password::verify(&form.password, &user.password_hash)
        .map_err(|_| AppError::Unauthorized)?;

    let session_token = auth::session::mint_token();
    let csrf = auth::session::mint_token();
    let lifetime = Duration::seconds(state.config.session_lifetime_seconds);
    let expires_at =
        OffsetDateTime::now_utc().unix_timestamp() + state.config.session_lifetime_seconds;

    db::sessions::create(&state.pool, &session_token, user.id, &csrf, expires_at).await?;
    state.login_limiter.reset(&email_key);

    let session_c = auth::session::session_cookie(&session_token, lifetime).to_string();
    let csrf_c = auth::session::csrf_cookie(&csrf, lifetime).to_string();

    let mut headers = HeaderMap::new();
    headers.append(SET_COOKIE, session_c.parse().unwrap());
    headers.append(SET_COOKIE, csrf_c.parse().unwrap());

    Ok((
        StatusCode::OK,
        headers,
        Json(LoginOk {
            user_id: user.id,
            csrf_token: csrf,
        }),
    )
        .into_response())
}

/// 429 response matching this handler's JSON error style, with a
/// `Retry-After` header naming the throttle window in seconds.
fn too_many_requests(window_secs: u64) -> axum::response::Response {
    let mut res = (
        StatusCode::TOO_MANY_REQUESTS,
        Json(serde_json::json!({
            "error": "too many login attempts, try again later",
            "status": StatusCode::TOO_MANY_REQUESTS.as_u16(),
        })),
    )
        .into_response();
    if let Ok(v) = HeaderValue::from_str(&window_secs.to_string()) {
        res.headers_mut().insert(axum::http::header::RETRY_AFTER, v);
    }
    res
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
        let hash = auth::password::hash("hunter2").unwrap();
        db::users::bootstrap_admin(&pool, "admin@example.com", &hash)
            .await
            .unwrap();
        let cfg = crate::config::Config::default();
        AppState::new(pool, cfg, vec![0u8; 32])
    }

    fn router_under_test(state: AppState) -> axum::Router {
        super::router().with_state(state)
    }

    #[tokio::test]
    async fn good_credentials_set_cookies() {
        let app = router_under_test(state().await);
        let body = serde_json::to_vec(&serde_json::json!({
            "email": "admin@example.com",
            "password": "hunter2"
        }))
        .unwrap();
        let res = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/login")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let set_cookie_count = res.headers().get_all(SET_COOKIE).iter().count();
        assert_eq!(set_cookie_count, 2);
    }

    #[tokio::test]
    async fn bad_password_unauthorized() {
        let app = router_under_test(state().await);
        let body = serde_json::to_vec(&serde_json::json!({
            "email": "admin@example.com",
            "password": "nope"
        }))
        .unwrap();
        let res = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/login")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn unknown_user_unauthorized() {
        let app = router_under_test(state().await);
        let body = serde_json::to_vec(&serde_json::json!({
            "email": "ghost@example.com",
            "password": "x"
        }))
        .unwrap();
        let res = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/login")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn eleventh_attempt_for_same_email_is_throttled() {
        let app = router_under_test(state().await);
        let body = serde_json::to_vec(&serde_json::json!({
            "email": "admin@example.com",
            "password": "nope"
        }))
        .unwrap();

        for _ in 0..10 {
            let res = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/login")
                        .header("content-type", "application/json")
                        .body(Body::from(body.clone()))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
        }

        let res = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/login")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::TOO_MANY_REQUESTS);
        let retry_after = res
            .headers()
            .get(axum::http::header::RETRY_AFTER)
            .expect("Retry-After header");
        assert_eq!(retry_after, "900");
    }
}
