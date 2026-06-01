//! POST /admin/posts/:id — htmx field-level saves.
//!
//! Accepts a `SaveForm` with option fields; only the fields that were sent in
//! this request are forwarded to `db::posts::update_fields`. `body_md` is
//! re-rendered to `body_html` via `content::render` so the persisted HTML
//! stays in lockstep with the source markdown (spec §4.2 invariant).
//!
//! SEO fields (`meta_description`, `og_image`, `canonical_url`,
//! `twitter_card`) and `series` / `series_order` are merged into the post's
//! existing `meta_json` blob so no pre-existing keys are clobbered.
//!
//! CSRF + auth are validated upstream by the admin router middleware stack.

use askama::Template;
use askama_axum::IntoResponse;
use axum::extract::{Path, State};
use axum::Form;
use db::posts::{self, PostUpdate};
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
    pub subtitle: Option<String>,
    #[serde(default)]
    pub excerpt: Option<String>,
    #[serde(default)]
    pub cover_image: Option<String>,
    #[serde(default)]
    pub tags_csv: Option<String>,
    #[serde(default)]
    pub body_md: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub scheduled_for: Option<String>,
    // SEO / meta_json fields
    #[serde(default)]
    pub series: Option<String>,
    #[serde(default)]
    pub meta_description: Option<String>,
    #[serde(default)]
    pub og_image: Option<String>,
    #[serde(default)]
    pub canonical_url: Option<String>,
    #[serde(default)]
    pub twitter_card: Option<String>,
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
    let mut update = PostUpdate::default();

    if let Some(v) = form.title.as_ref().filter(|s| !s.is_empty()) {
        update.title = Some(v.clone());
    }
    if let Some(v) = form.slug.as_ref().filter(|s| !s.is_empty()) {
        update.slug = Some(slugify(v));
    }
    if form.subtitle.is_some() {
        update.subtitle = form.subtitle.clone();
    }
    if form.excerpt.is_some() {
        update.excerpt = form.excerpt.clone();
    }
    if form.cover_image.is_some() {
        update.cover_image = form.cover_image.clone();
    }
    if form.tags_csv.is_some() {
        update.tags_csv = form.tags_csv.clone();
    }
    if let Some(s) = form
        .status
        .as_ref()
        .filter(|s| matches!(s.as_str(), "draft" | "scheduled" | "published"))
    {
        update.status = Some(s.clone());
    }
    if let Some(s) = form.scheduled_for.as_ref() {
        update.scheduled_for = Some(if s.is_empty() {
            None
        } else {
            s.parse::<i64>().ok()
        });
    }
    if let Some(md) = form.body_md.as_ref() {
        let out = content::render(md).map_err(|e| AppError::BadRequest(e.to_string()))?;
        update.body_md = Some(md.clone());
        update.body_html = Some(out.html);
        update.toc_json = Some(
            serde_json::to_string(&out.toc).unwrap_or_else(|_| "[]".into()),
        );
        update.reading_minutes = Some(out.reading_minutes);
    }

    // --- Merge SEO / series fields into meta_json ---
    // Any of the five keys being present in the form triggers a meta_json
    // update.  We read the existing blob first so we never clobber keys that
    // this request didn't touch (e.g. series_order set by a different tool).
    let meta_fields_present = form.series.is_some()
        || form.meta_description.is_some()
        || form.og_image.is_some()
        || form.canonical_url.is_some()
        || form.twitter_card.is_some();

    if meta_fields_present {
        // Load existing meta_json from DB so we don't lose unrelated keys.
        let existing_raw: Option<String> =
            sqlx::query_scalar("SELECT meta_json FROM posts WHERE id = ?")
                .bind(id)
                .fetch_optional(&state.pool)
                .await?
                .flatten();

        let mut map: serde_json::Map<String, serde_json::Value> = existing_raw
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default();

        // Each field: if Some("") → remove key; Some(non-empty) → set; None → leave alone.
        set_or_remove(&mut map, "series", form.series.as_deref());
        set_or_remove(&mut map, "meta_description", form.meta_description.as_deref());
        set_or_remove(&mut map, "og_image", form.og_image.as_deref());
        set_or_remove(&mut map, "canonical_url", form.canonical_url.as_deref());
        set_or_remove(&mut map, "twitter_card", form.twitter_card.as_deref());

        update.meta_json = Some(serde_json::to_string(&map).unwrap_or_else(|_| "{}".into()));
    }

    posts::update_fields(&state.pool, id, &update).await?;

    Ok(FlashTpl {
        flash: Some("Saved.".into()),
        flash_kind: "ok".into(),
    })
}

/// Upsert or delete a key in a JSON map based on the submitted value.
/// - `Some("")`  → remove the key (clear the field)
/// - `Some(v)`   → set the key to `v`
/// - `None`      → leave the key untouched
fn set_or_remove(
    map: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
    value: Option<&str>,
) {
    match value {
        Some("") => {
            map.remove(key);
        }
        Some(v) => {
            map.insert(key.to_owned(), serde_json::Value::String(v.to_owned()));
        }
        None => {}
    }
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
        out.push_str("post");
    }
    out
}

#[cfg(test)]
mod tests {
    use crate::config::Config;
    use axum::body::{to_bytes, Body};
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

    /// Seed a draft post (user must already exist in DB).
    async fn seed_draft_post(state: &crate::state::AppState, slug: &str, meta_json: &str) -> i64 {
        sqlx::query_scalar::<_, i64>(
            r#"
            INSERT INTO posts (slug, title, status, author_id,
                               updated_at, created_at, body_md, body_html,
                               meta_json, assets_json)
            VALUES (?, 'Test Post', 'draft', 1, 0, 0, '# x', '<h1>x</h1>', ?, '[]')
            RETURNING id
            "#,
        )
        .bind(slug)
        .bind(meta_json)
        .fetch_one(&state.pool)
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn seo_fields_round_trip_into_meta_json() {
        let (app, state) = test_app().await;
        let (sid, csrf) = seed_admin_session(&state).await;
        // seed_admin_session creates the user; now seed the post.
        let post_id = seed_draft_post(&state, "seo-roundtrip", "{}").await;

        let cookie = format!(
            "{}={}; {}={}",
            auth::session::SESSION_COOKIE,
            sid,
            auth::session::CSRF_COOKIE,
            csrf
        );
        let body_str =
            "meta_description=A+great+post&og_image=https%3A%2F%2Fcdn.example.com%2Fog.png\
             &canonical_url=https%3A%2F%2Fexample.com%2Fposts%2Ftest&twitter_card=summary_large_image"
                .to_string();

        let res = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/admin/posts/{post_id}"))
                    .header(header::COOKIE, &cookie)
                    .header("x-csrf-token", &csrf)
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .body(Body::from(body_str))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(res.status(), StatusCode::OK);

        // Verify meta_json was persisted correctly.
        let raw: Option<String> =
            sqlx::query_scalar("SELECT meta_json FROM posts WHERE id = ?")
                .bind(post_id)
                .fetch_one(&state.pool)
                .await
                .unwrap();

        let meta: serde_json::Value =
            serde_json::from_str(raw.as_deref().unwrap_or("{}")).unwrap();
        assert_eq!(meta["meta_description"], "A great post");
        assert_eq!(meta["og_image"], "https://cdn.example.com/og.png");
        assert_eq!(meta["canonical_url"], "https://example.com/posts/test");
        assert_eq!(meta["twitter_card"], "summary_large_image");
    }

    #[tokio::test]
    async fn seo_fields_do_not_clobber_series() {
        let (app, state) = test_app().await;
        let (sid, csrf) = seed_admin_session(&state).await;

        // Seed a post with existing series in meta_json.
        let post_id =
            seed_draft_post(&state, "series-preserve", r#"{"series":"my-series","series_order":2}"#)
                .await;

        let cookie = format!(
            "{}={}; {}={}",
            auth::session::SESSION_COOKIE,
            sid,
            auth::session::CSRF_COOKIE,
            csrf
        );
        // POST only the meta_description — series keys must survive.
        let body_str = "meta_description=Desc+only".to_string();

        let res = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/admin/posts/{post_id}"))
                    .header(header::COOKIE, &cookie)
                    .header("x-csrf-token", &csrf)
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .body(Body::from(body_str))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(res.status(), StatusCode::OK);

        let raw: Option<String> =
            sqlx::query_scalar("SELECT meta_json FROM posts WHERE id = ?")
                .bind(post_id)
                .fetch_one(&state.pool)
                .await
                .unwrap();

        let meta: serde_json::Value =
            serde_json::from_str(raw.as_deref().unwrap_or("{}")).unwrap();
        // SEO field saved.
        assert_eq!(meta["meta_description"], "Desc only");
        // Series keys preserved — not clobbered by the SEO-only save.
        assert_eq!(meta["series"], "my-series");
        assert_eq!(meta["series_order"], 2);
    }

    #[tokio::test]
    async fn edit_get_prefills_seo_fields_from_meta_json() {
        let (app, state) = test_app().await;
        let (sid, csrf) = seed_admin_session(&state).await;
        let post_id = seed_draft_post(
            &state,
            "prefill-test",
            r#"{"meta_description":"hello","series":"foo","og_image":"https://img.png"}"#,
        )
        .await;

        let cookie = format!(
            "{}={}; {}={}",
            auth::session::SESSION_COOKIE,
            sid,
            auth::session::CSRF_COOKIE,
            csrf
        );

        let res = app
            .oneshot(
                Request::builder()
                    .uri(format!("/admin/posts/{post_id}"))
                    .header(header::COOKIE, &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(res.status(), StatusCode::OK);
        let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let body = std::str::from_utf8(&bytes).unwrap();

        // Template should pre-fill the SEO textarea and inputs with meta_json values.
        assert!(
            body.contains("hello"),
            "meta_description not pre-filled: {body}"
        );
        assert!(body.contains("foo"), "series not pre-filled: {body}");
        assert!(
            body.contains("https://img.png"),
            "og_image not pre-filled: {body}"
        );
    }
}
