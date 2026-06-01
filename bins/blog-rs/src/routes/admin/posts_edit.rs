//! GET /admin/posts/:id — render the editor form for an existing post.
//!
//! Auth + CSRF are handled by the `auth_required` / `csrf` middleware layers
//! in `routes::admin::router`; here we just trust the `SessionCtx` extension
//! and read the post.

use askama::Template;
use askama_axum::IntoResponse;
use axum::extract::{Path, State};
use axum::Extension;

use crate::error::AppError;
use crate::middleware::auth_required::SessionCtx;
use crate::state::AppState;

#[derive(Template)]
#[template(path = "admin/posts_edit.html")]
#[allow(dead_code)] // flash + flash_kind reserved for Plan 1e flash messaging
struct EditTpl {
    csrf: String,
    nav: &'static str,
    page_title: String,
    flash: Option<String>,
    flash_kind: String,
    id: i64,
    title: String,
    slug: String,
    subtitle: String,
    excerpt: String,
    cover_image: String,
    tags_csv: String,
    body_md: String,
    body_html: String,
    status: String,
    scheduled_for: String,
    // meta_json-derived fields
    series: String,
    meta_description: String,
    og_image: String,
    canonical_url: String,
    twitter_card: String,
}

pub async fn handler(
    State(state): State<AppState>,
    Extension(session): Extension<SessionCtx>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    let post = db::posts::find_by_id(&state.pool, id).await.map_err(|e| {
        if matches!(e, db::DbError::NotFound) {
            AppError::NotFound
        } else {
            AppError::from(e)
        }
    })?;

    let tags: Vec<String> = sqlx::query_scalar(
        "SELECT t.name FROM tags t \
         JOIN post_tags pt ON pt.tag_id = t.id \
         WHERE pt.post_id = ? ORDER BY t.name",
    )
    .bind(id)
    .fetch_all(&state.pool)
    .await?;

    // Parse meta_json once to extract SEO fields + series.
    let meta: serde_json::Value = post
        .meta_json
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or(serde_json::Value::Object(Default::default()));

    let str_field = |key: &str| -> String {
        meta.get(key)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned()
    };

    Ok(EditTpl {
        csrf: session.csrf_token.clone(),
        nav: "posts",
        page_title: format!("Edit · {}", &post.title),
        flash: None,
        flash_kind: String::new(),
        id: post.id,
        title: post.title,
        slug: post.slug,
        subtitle: post.subtitle.unwrap_or_default(),
        excerpt: post.excerpt.unwrap_or_default(),
        cover_image: post.cover_image.unwrap_or_default(),
        tags_csv: tags.join(", "),
        body_md: post.body_md,
        body_html: post.body_html,
        status: post.status,
        scheduled_for: post
            .scheduled_for
            .map(|t| t.to_string())
            .unwrap_or_default(),
        series: str_field("series"),
        meta_description: str_field("meta_description"),
        og_image: str_field("og_image"),
        canonical_url: str_field("canonical_url"),
        twitter_card: str_field("twitter_card"),
    })
}
