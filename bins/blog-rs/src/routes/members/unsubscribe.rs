//! Public unsubscribe route: GET shows an interstitial confirm button,
//! POST performs the unsubscribe. To support mail clients that auto-fetch
//! `List-Unsubscribe` URLs (RFC 8058 one-click), GET with `?confirm=1`
//! also performs the unsubscribe immediately.
//!
//! No login required — authorization comes from the HMAC token in the path.

use askama::Template;
use askama_axum::IntoResponse;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use serde::Deserialize;

use crate::state::AppState;
use crate::tokens::Purpose;
use crate::view::{AssetTag, SiteCtx};
use db::members;

#[derive(Template)]
#[template(path = "members/unsubscribe_form.html")]
pub struct UnsubForm<'a> {
    pub site: SiteCtx,
    pub asset_tags: Vec<AssetTag>,
    pub site_title: &'a str,
    pub email: &'a str,
    pub token: &'a str,
}

#[derive(Template)]
#[template(path = "members/unsubscribe_done.html")]
pub struct UnsubDone<'a> {
    pub site: SiteCtx,
    pub asset_tags: Vec<AssetTag>,
    pub site_title: &'a str,
    pub email: &'a str,
}

#[derive(Debug, Deserialize, Default)]
pub struct OneClickQs {
    #[serde(default)]
    pub confirm: u8,
}

fn site_ctx(st: &AppState) -> SiteCtx {
    SiteCtx {
        title: st.site.site_title.clone(),
        base_url: st.site.base_url.clone(),
        description: String::new(),
    }
}

/// `GET /unsubscribe/:token` — shows the interstitial confirm form. If
/// `?confirm=1` is present (RFC 8058 one-click path), performs the
/// unsubscribe and renders the done page directly.
pub async fn show(
    State(st): State<AppState>,
    Path(token): Path<String>,
    Query(qs): Query<OneClickQs>,
) -> axum::response::Response {
    let payload = match st.tokens.verify(&token, Purpose::Unsubscribe) {
        Ok(p) => p,
        Err(_) => {
            return (StatusCode::BAD_REQUEST, "Invalid or expired link.").into_response();
        }
    };
    let member = match members::find_by_id(&st.pool, payload.member_id as i64).await {
        Ok(m) => m,
        Err(_) => {
            // Collapse unknown-member into the same response as bad-token so
            // an attacker cannot distinguish "token forged" from "token valid
            // for a deleted account".
            return (StatusCode::BAD_REQUEST, "Invalid or expired link.").into_response();
        }
    };

    if qs.confirm == 1 {
        let _ = members::unsubscribe(&st.pool, member.id).await;
        let done = UnsubDone {
            site: site_ctx(&st),
            asset_tags: Vec::new(),
            site_title: &st.site.site_title,
            email: &member.email,
        };
        return done.into_response();
    }

    let form = UnsubForm {
        site: site_ctx(&st),
        asset_tags: Vec::new(),
        site_title: &st.site.site_title,
        email: &member.email,
        token: &token,
    };
    form.into_response()
}

/// `POST /unsubscribe/:token` — performs the unsubscribe and renders the
/// confirmation page. Idempotent: re-submitting the same token is harmless
/// because `members::unsubscribe` only flips the row.
pub async fn submit(
    State(st): State<AppState>,
    Path(token): Path<String>,
) -> axum::response::Response {
    let payload = match st.tokens.verify(&token, Purpose::Unsubscribe) {
        Ok(p) => p,
        Err(_) => {
            return (StatusCode::BAD_REQUEST, "Invalid or expired link.").into_response();
        }
    };
    let member = match members::find_by_id(&st.pool, payload.member_id as i64).await {
        Ok(m) => m,
        Err(_) => {
            // Collapse unknown-member into the same response as bad-token so
            // an attacker cannot distinguish "token forged" from "token valid
            // for a deleted account".
            return (StatusCode::BAD_REQUEST, "Invalid or expired link.").into_response();
        }
    };
    let _ = members::unsubscribe(&st.pool, member.id).await;
    let done = UnsubDone {
        site: site_ctx(&st),
        asset_tags: Vec::new(),
        site_title: &st.site.site_title,
        email: &member.email,
    };
    done.into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use axum::body::{to_bytes, Body};
    use axum::http::Request;
    use axum::routing::get;
    use axum::Router;
    use tower::ServiceExt;

    async fn test_app() -> (Router, AppState) {
        let pool = db::test_support::fresh_pool().await;
        let state = AppState::new(pool, Config::default(), vec![0u8; 32]);
        let app = Router::new()
            .route("/unsubscribe/:token", get(show).post(submit))
            .with_state(state.clone());
        (app, state)
    }

    async fn seed_member(st: &AppState, email: &str) -> i64 {
        members::insert_fixture(&st.pool, email, Some(1_700_000_000), None)
            .await
            .unwrap()
    }

    async fn body_string(res: axum::response::Response) -> (StatusCode, String) {
        let status = res.status();
        let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
        (status, String::from_utf8(bytes.to_vec()).unwrap())
    }

    #[tokio::test]
    async fn get_shows_interstitial_form_without_unsubscribing() {
        let (app, st) = test_app().await;
        let id = seed_member(&st, "alice@example.com").await;
        let token = st.tokens.issue(id as u32, Purpose::Unsubscribe).unwrap();

        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/unsubscribe/{token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let (status, body) = body_string(res).await;
        assert_eq!(status, StatusCode::OK);
        assert!(body.contains("alice@example.com"), "email missing: {body}");
        assert!(body.contains("<form"), "form missing: {body}");

        // Confirm row was NOT mutated by the GET render.
        let m = members::find_by_id(&st.pool, id).await.unwrap();
        assert!(m.unsubscribed_at.is_none());
    }

    #[tokio::test]
    async fn get_with_confirm_one_click_unsubscribes() {
        let (app, st) = test_app().await;
        let id = seed_member(&st, "bob@example.com").await;
        let token = st.tokens.issue(id as u32, Purpose::Unsubscribe).unwrap();

        let res = app
            .oneshot(
                Request::builder()
                    .uri(format!("/unsubscribe/{token}?confirm=1"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let (status, body) = body_string(res).await;
        assert_eq!(status, StatusCode::OK);
        assert!(body.contains("unsubscribed"), "done page missing: {body}");

        let m = members::find_by_id(&st.pool, id).await.unwrap();
        assert!(m.unsubscribed_at.is_some());
    }

    #[tokio::test]
    async fn post_unsubscribes_and_is_idempotent() {
        let (app, st) = test_app().await;
        let id = seed_member(&st, "carol@example.com").await;
        let token = st.tokens.issue(id as u32, Purpose::Unsubscribe).unwrap();

        let req = || {
            Request::builder()
                .method("POST")
                .uri(format!("/unsubscribe/{token}"))
                .body(Body::empty())
                .unwrap()
        };

        let res1 = app.clone().oneshot(req()).await.unwrap();
        assert_eq!(res1.status(), StatusCode::OK);
        let m1 = members::find_by_id(&st.pool, id).await.unwrap();
        let first_ts = m1.unsubscribed_at.expect("unsubscribed_at set");

        // Second POST: still 200, row still unsubscribed (idempotent).
        let res2 = app.oneshot(req()).await.unwrap();
        assert_eq!(res2.status(), StatusCode::OK);
        let m2 = members::find_by_id(&st.pool, id).await.unwrap();
        assert!(m2.unsubscribed_at.is_some());
        // Current `members::unsubscribe` overwrites the timestamp; we don't
        // assert equality with `first_ts` here, only that the row remains
        // unsubscribed. (The plan notes a future COALESCE migration.)
        let _ = first_ts;
    }

    #[tokio::test]
    async fn invalid_token_rejected_with_400() {
        let (app, _st) = test_app().await;
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/unsubscribe/not-a-real-token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn wrong_purpose_token_rejected() {
        let (app, st) = test_app().await;
        let id = seed_member(&st, "dave@example.com").await;
        // Issue a confirm token, then try to use it for unsubscribe.
        let token = st.tokens.issue(id as u32, Purpose::Confirm).unwrap();
        let res = app
            .oneshot(
                Request::builder()
                    .uri(format!("/unsubscribe/{token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    }
}
