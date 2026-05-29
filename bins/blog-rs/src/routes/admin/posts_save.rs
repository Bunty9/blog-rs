//! POST /admin/posts/:id — htmx field-level saves.
//!
//! Accepts a `SaveForm` with option fields; only the fields that were sent in
//! this request are forwarded to `db::posts::update_fields`. `body_md` is
//! re-rendered to `body_html` via `content::render` so the persisted HTML
//! stays in lockstep with the source markdown (spec §4.2 invariant).
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
    }

    posts::update_fields(&state.pool, id, &update).await?;

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
        out.push_str("post");
    }
    out
}
