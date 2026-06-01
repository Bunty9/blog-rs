//! GET /admin/pages — list all static pages.
//!
//! Auth + CSRF live in the surrounding middleware.

use askama::Template;
use askama_axum::IntoResponse;
use axum::extract::State;
use axum::Extension;
use db::pages::Page;

use crate::error::AppError;
use crate::middleware::auth_required::SessionCtx;
use crate::state::AppState;

#[derive(Template)]
#[template(path = "admin/pages_list.html")]
#[allow(dead_code)] // flash + flash_kind reserved for flash messaging
struct PagesListTpl {
    csrf: String,
    nav: &'static str,
    page_title: &'static str,
    flash: Option<String>,
    flash_kind: String,
    rows: Vec<Page>,
}

pub async fn handler(
    State(state): State<AppState>,
    Extension(session): Extension<SessionCtx>,
) -> Result<impl IntoResponse, AppError> {
    let rows = db::pages::list_all(&state.pool).await?;
    Ok(PagesListTpl {
        csrf: session.csrf_token.clone(),
        nav: "pages",
        page_title: "Pages",
        flash: None,
        flash_kind: String::new(),
        rows,
    })
}

#[cfg(test)]
mod tests {
    use crate::config::Config;
    use crate::state::AppState;
    use axum::body::Body;
    use axum::http::{header, Request, StatusCode};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    async fn test_app() -> (axum::Router, AppState) {
        let pool = db::test_support::fresh_pool().await;
        let state = AppState::new(pool.clone(), Config::default(), vec![0u8; 32]);
        let app = crate::routes::router(state.clone());
        (app, state)
    }

    async fn seed_admin_session(state: &AppState) -> String {
        let hash = auth::password::hash("hunter2").unwrap();
        db::users::bootstrap_admin(&state.pool, "admin@test.com", &hash)
            .await
            .unwrap();
        let user_id = db::users::find_by_email(&state.pool, "admin@test.com")
            .await
            .unwrap()
            .id;
        let session_token = auth::session::mint_token();
        let csrf = auth::session::mint_token();
        let expires = time::OffsetDateTime::now_utc().unix_timestamp() + 3600;
        db::sessions::create(&state.pool, &session_token, user_id, &csrf, expires)
            .await
            .unwrap();
        session_token
    }

    #[tokio::test]
    async fn unauthenticated_returns_401() {
        let (app, _state) = test_app().await;
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/admin/pages")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        // auth_required middleware returns 401 for unauthenticated requests
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn seeded_page_appears_in_list_when_authed() {
        let (app, state) = test_app().await;
        let sid = seed_admin_session(&state).await;

        // Seed a page
        db::pages::create(
            &state.pool,
            db::pages::NewPage {
                slug: "about",
                title: "About the blog",
                body_md: "# About",
                body_html: "<h1>About</h1>",
                toc_json: "[]",
                meta_json: None,
                status: "published",
            },
        )
        .await
        .unwrap();

        let cookie = format!("{}={}", auth::session::SESSION_COOKIE, sid);
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/admin/pages")
                    .header(header::COOKIE, cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let body = std::str::from_utf8(&bytes).unwrap();
        assert!(
            body.contains("About the blog"),
            "page title missing from list: {body}"
        );
    }
}
