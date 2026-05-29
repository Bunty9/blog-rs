//! POST /admin/posts/:id/preview — live preview htmx fragment.
//!
//! Takes the in-flight `body_md` from the editor, runs it through
//! `content::render`, and returns a rendered HTML fragment (or a flash error
//! if the render failed). No DB writes; the `:id` is only used for routing
//! symmetry with the rest of the post edit surface.
//!
//! CSRF is enforced by the `middleware::csrf::layer` already wrapping every
//! mutating admin route in `routes::admin::mod::router`, so this handler does
//! not re-check it.

use askama::Template;
use askama_axum::IntoResponse;
use axum::extract::{Path, State};
use axum::Form;
use serde::Deserialize;

use crate::error::AppError;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct PreviewForm {
    pub body_md: String,
}

#[derive(Template)]
#[template(path = "admin/posts_preview_pane.html")]
struct PreviewTpl {
    html: String,
    error: Option<String>,
}

pub async fn handler(
    State(_state): State<AppState>,
    Path(_id): Path<i64>,
    Form(form): Form<PreviewForm>,
) -> Result<axum::response::Response, AppError> {
    let (html, error) = match content::render(&form.body_md) {
        Ok(out) => (out.html, None),
        Err(e) => (String::new(), Some(e.to_string())),
    };
    Ok(PreviewTpl { html, error }.into_response())
}
