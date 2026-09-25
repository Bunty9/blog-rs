//! Public member signup. GET renders the form; POST validates, inserts (or
//! resurrects) the member row, and synchronously sends the confirm email.
//!
//! Synchronous send is intentional: the user is staring at their inbox; we
//! accept the SMTP round-trip latency cost on this single endpoint to keep
//! "click subscribe → click link" UX tight. Newsletter post fan-out is the
//! reverse: enqueue-only, drained by the background worker.

use askama::Template;
use askama_axum::IntoResponse;
use axum::extract::{ConnectInfo, Form, State};
use axum::http::{HeaderValue, StatusCode};
use axum::response::Response;
use cookie::Cookie;
use serde::Deserialize;
use std::net::SocketAddr;

use crate::rate_limit;
use crate::state::AppState;
use crate::view::{AssetTag, SiteCtx};
use db::members;

/// Combined view-model for both the empty form and the "pending — check inbox"
/// view. We render a single template with a `pending` flag so the URL stays
/// `/signup` on POST (no redirect dance, no flash store).
#[derive(Template)]
#[template(path = "members/signup.html")]
pub struct SignupPage<'a> {
    pub site: SiteCtx,
    pub asset_tags: Vec<AssetTag>,
    pub nav: &'static str,
    pub site_title: &'a str,
    pub csrf_token: &'a str,
    pub email: &'a str,
    pub error: Option<&'a str>,
    pub pending: bool,
    pub ttl_hours: u32,
}

#[derive(Debug, Deserialize)]
pub struct Input {
    pub email: String,
    pub csrf_token: String,
}

pub async fn show(State(st): State<AppState>, headers: axum::http::HeaderMap) -> Response {
    // Mint a fresh CSRF token on every visit that arrives without one. The
    // submit handler requires the cookie unconditionally, so a real browser
    // visit must always leave with one set.
    let (csrf, set_cookie) = match csrf_from_cookie(&headers) {
        Some(v) if !v.is_empty() => (v, None),
        _ => {
            let token = auth::session::mint_token();
            let lifetime = time::Duration::seconds(st.config.session_lifetime_seconds);
            let cookie = auth::session::csrf_cookie(&token, lifetime);
            (token, Some(cookie.to_string()))
        }
    };
    let mut res = SignupPage {
        site: SiteCtx::placeholder(),
        asset_tags: Vec::new(),
        nav: "",
        site_title: &st.site.site_title,
        csrf_token: &csrf,
        email: "",
        error: None,
        pending: false,
        ttl_hours: 0,
    }
    .into_response();
    if let Some(c) = set_cookie {
        if let Ok(v) = axum::http::HeaderValue::from_str(&c) {
            res.headers_mut().append(axum::http::header::SET_COOKIE, v);
        }
    }
    res
}

pub async fn submit(
    State(st): State<AppState>,
    connect_info: Option<ConnectInfo<SocketAddr>>,
    headers: axum::http::HeaderMap,
    Form(input): Form<Input>,
) -> Response {
    // Double-submit CSRF: cookie value must match form field. The cookie is
    // seeded by GET /signup before the user ever submits, so a real browser
    // round-trip always has it. Anonymous POSTs that arrive without the
    // cookie are either cross-origin attacks or scripts that bypassed the
    // form render; either way, reject. Synchronous SMTP send on the happy
    // path makes this endpoint cheap to weaponise as an email-spammer, so
    // CSRF is unconditional here.
    let cookie_csrf = match csrf_from_cookie(&headers) {
        Some(v) if !v.is_empty() => v,
        _ => return (StatusCode::FORBIDDEN, "CSRF cookie missing").into_response(),
    };
    if auth::csrf::validate(&cookie_csrf, &input.csrf_token).is_err() {
        return (StatusCode::FORBIDDEN, "CSRF validation failed").into_response();
    }

    // Only count requests that pass CSRF: only those can actually trigger a
    // send below, so this is the point that matters for spam throttling.
    let ip = rate_limit::client_ip(&headers, connect_info.map(|ci| ci.0));
    if !st.signup_limiter.check(&format!("signup-ip:{ip}")) {
        tracing::warn!(ip = %ip, "signup rate limited");
        return too_many_requests(st.signup_limiter.window_secs());
    }

    if !is_valid_email(&input.email) {
        return SignupPage {
            site: SiteCtx::placeholder(),
            asset_tags: Vec::new(),
            nav: "",
            site_title: &st.site.site_title,
            csrf_token: &input.csrf_token,
            email: &input.email,
            error: Some("Please enter a valid email address."),
            pending: false,
            ttl_hours: 0,
        }
        .into_response();
    }

    let (member, outcome) = match members::signup(&st.pool, &input.email).await {
        Ok(v) => v,
        Err(e) => {
            tracing::error!(error = ?e, "signup db failure");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Internal error").into_response();
        }
    };

    // Already-confirmed users see the same "check your inbox" page (no oracle
    // about which addresses are subscribed) but no email is enqueued.
    let should_send = matches!(
        outcome,
        members::SignupOutcome::Created
            | members::SignupOutcome::AlreadyPending
            | members::SignupOutcome::Resubscribed
    );
    if should_send {
        // The outbox worker renders and sends the confirm mail; sending it
        // here as well delivered every confirm email twice.
        if let Err(e) = members::enqueue_confirm(&st.pool, member.id).await {
            tracing::error!(error = ?e, "confirm enqueue failed");
        }
    }

    let ttl_hours = (st.tokens.ttl() / 3600).max(1);
    SignupPage {
        site: SiteCtx::placeholder(),
        asset_tags: Vec::new(),
        nav: "",
        site_title: &st.site.site_title,
        csrf_token: &input.csrf_token,
        email: &member.email,
        error: None,
        pending: true,
        ttl_hours,
    }
    .into_response()
}

fn csrf_from_cookie(headers: &axum::http::HeaderMap) -> Option<String> {
    headers
        .get(axum::http::header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| {
            Cookie::split_parse(s)
                .filter_map(|c| c.ok())
                .find(|c| c.name() == auth::session::CSRF_COOKIE)
                .map(|c| c.value().to_string())
        })
}

/// 429 response with a `Retry-After` header naming the throttle window in
/// seconds, in this handler's existing plain-text error style.
fn too_many_requests(window_secs: u64) -> Response {
    let mut res = (
        StatusCode::TOO_MANY_REQUESTS,
        "Too many signups, try again later",
    )
        .into_response();
    if let Ok(v) = HeaderValue::from_str(&window_secs.to_string()) {
        res.headers_mut().insert(axum::http::header::RETRY_AFTER, v);
    }
    res
}

fn is_valid_email(s: &str) -> bool {
    // Minimal RFC-ish check: one '@', non-empty local and domain, no whitespace,
    // at least one '.' in the domain part.
    let s = s.trim();
    let mut parts = s.split('@');
    let local = parts.next().unwrap_or("");
    let domain = parts.next().unwrap_or("");
    if parts.next().is_some() {
        return false;
    }
    if local.is_empty() || domain.is_empty() {
        return false;
    }
    if local.chars().any(|c| c.is_whitespace()) {
        return false;
    }
    if domain.chars().any(|c| c.is_whitespace()) {
        return false;
    }
    domain.contains('.')
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use axum::routing::get;
    use db::test_support::fresh_pool;
    use tower::ServiceExt;

    #[test]
    fn accepts_well_formed() {
        assert!(is_valid_email("a@b.co"));
        assert!(is_valid_email("a.b+c@example.com"));
    }

    #[test]
    fn rejects_garbage() {
        assert!(!is_valid_email("a"));
        assert!(!is_valid_email("a@b"));
        assert!(!is_valid_email("a@@b.co"));
        assert!(!is_valid_email(""));
        assert!(!is_valid_email("a @b.co"));
    }

    async fn state() -> AppState {
        let pool = fresh_pool().await;
        let cfg = crate::config::Config::default();
        AppState::new(pool, cfg, vec![0u8; 32])
    }

    fn router_under_test(state: AppState) -> axum::Router {
        axum::Router::new()
            .route("/signup", get(show).post(submit))
            .with_state(state)
    }

    const CSRF: &str = "test-csrf-token-0123456789";

    fn signup_request(email: &str, cf_connecting_ip: Option<&str>) -> Request<Body> {
        let body = format!("email={}&csrf_token={}", urlencoding::encode(email), CSRF);
        let mut builder = Request::builder()
            .method("POST")
            .uri("/signup")
            .header("content-type", "application/x-www-form-urlencoded")
            .header("cookie", format!("XSRF-TOKEN={CSRF}"));
        if let Some(ip) = cf_connecting_ip {
            builder = builder.header("cf-connecting-ip", ip);
        }
        builder.body(Body::from(body)).unwrap()
    }

    #[tokio::test]
    async fn valid_signup_shows_pending() {
        let app = router_under_test(state().await);
        let res = app
            .oneshot(signup_request("reader@example.com", None))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn sixth_signup_from_same_ip_is_throttled() {
        let app = router_under_test(state().await);

        for i in 0..5 {
            let res = app
                .clone()
                .oneshot(signup_request(
                    &format!("reader{i}@example.com"),
                    Some("5.6.7.8"),
                ))
                .await
                .unwrap();
            assert_eq!(res.status(), StatusCode::OK);
        }

        let res = app
            .oneshot(signup_request("reader6@example.com", Some("5.6.7.8")))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::TOO_MANY_REQUESTS);
        let retry_after = res
            .headers()
            .get(axum::http::header::RETRY_AFTER)
            .expect("Retry-After header");
        assert_eq!(retry_after, "3600");
    }
}
