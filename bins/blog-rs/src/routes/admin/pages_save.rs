//! POST /admin/pages/:id — save a static page.
//!
//! Accepts a `SaveForm`; re-renders body_md to body_html via `content::render`
//! so the persisted HTML stays in lockstep with the source markdown.
//!
//! CSRF + auth are validated upstream by the admin router middleware stack.

use askama::Template;
use askama_axum::IntoResponse;
use axum::extract::{Path, State};
use axum::Form;
use db::pages::{self, PageUpdate};
use serde::Deserialize;

use crate::error::AppError;
use crate::state::AppState;

#[derive(Debug, Deserialize, Default)]
pub struct SaveForm {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub slug: Option<String>,
    #[serde(default)]
    pub body_md: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
}

#[derive(Template)]
#[template(path = "admin/partials/flash.html")]
struct FlashTpl {
    flash: Option<String>,
    flash_kind: String,
}

pub async fn handler(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Form(form): Form<SaveForm>,
) -> Result<impl IntoResponse, AppError> {
    let mut update = PageUpdate::default();

    if let Some(v) = form.title.as_ref().filter(|s| !s.is_empty()) {
        update.title = Some(v.clone());
    }
    if let Some(v) = form.slug.as_ref().filter(|s| !s.is_empty()) {
        update.slug = Some(slugify(v));
    }
    if let Some(s) = form
        .status
        .as_ref()
        .filter(|s| matches!(s.as_str(), "draft" | "published"))
    {
        update.status = Some(s.clone());
    }
    if let Some(md) = form.body_md.as_ref() {
        let out = content::render(md).map_err(|e| AppError::BadRequest(e.to_string()))?;
        update.body_md = Some(md.clone());
        update.body_html = Some(out.html);
        update.toc_json = Some(serde_json::to_string(&out.toc).unwrap_or_else(|_| "[]".into()));
        update.assets_json =
            Some(serde_json::to_string(&out.assets).unwrap_or_else(|_| "[]".into()));
    }

    pages::update_fields(&state.pool, id, &update).await?;

    Ok(FlashTpl {
        flash: Some("Saved.".into()),
        flash_kind: "ok".into(),
    })
}

fn slugify(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_dash = false;
    for ch in s.chars().flat_map(|c| c.to_lowercase()) {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            prev_dash = false;
        } else if !prev_dash && !out.is_empty() {
            out.push('-');
            prev_dash = true;
        }
    }
    if out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() {
        out.push_str("page");
    }
    out
}

#[cfg(test)]
mod tests {
    use crate::config::Config;
    use axum::body::Body;
    use axum::http::{header, Request, StatusCode};
    use db::test_support::fresh_pool;
    use tower::ServiceExt;

    async fn test_app() -> (axum::Router, crate::state::AppState) {
        let pool = fresh_pool().await;
        let state = crate::state::AppState::new(pool, Config::default(), vec![0u8; 32]);
        let app = crate::routes::router(state.clone());
        (app, state)
    }

    async fn seed_admin_session(state: &crate::state::AppState) -> (String, String) {
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

    async fn seed_draft_page(state: &crate::state::AppState, slug: &str) -> i64 {
        db::pages::create(
            &state.pool,
            db::pages::NewPage {
                slug,
                title: "Test Page",
                body_md: "# x",
                body_html: "<h1>x</h1>",
                toc_json: "[]",
                meta_json: None,
                status: "draft",
                assets_json: "[]",
            },
        )
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn saving_body_refreshes_asset_manifest() {
        // Regression: static pages ignored the shortcode asset manifest, so
        // a page using a chart/animate/code-playground block shipped
        // without its CSS/JS.
        let (app, state) = test_app().await;
        let (sid, csrf) = seed_admin_session(&state).await;
        let page_id = seed_draft_page(&state, "assets-refresh").await;

        let body = format!(
            "body_md={}",
            urlencoding::encode(r#"{{< chart type="bar" data="[1,2]" >}}"#)
        );
        let res = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/admin/pages/{page_id}"))
                    .header(
                        header::COOKIE,
                        format!("{}={}", auth::session::SESSION_COOKIE, sid),
                    )
                    .header("x-csrf-token", &csrf)
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        let assets: String = sqlx::query_scalar("SELECT assets_json FROM pages WHERE id = ?")
            .bind(page_id)
            .fetch_one(&state.pool)
            .await
            .unwrap();
        assert_ne!(assets, "[]", "chart assets missing from manifest");
    }
}
