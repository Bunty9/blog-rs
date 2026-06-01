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
