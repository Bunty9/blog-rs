//! GET /admin/pages/:id/edit — render the editor form for an existing page.
//!
//! Auth + CSRF are handled by the `auth_required` / `csrf` middleware layers.

use askama::Template;
use askama_axum::IntoResponse;
use axum::extract::{Path, State};
use axum::Extension;

use crate::error::AppError;
use crate::middleware::auth_required::SessionCtx;
use crate::state::AppState;

#[derive(Template)]
#[template(path = "admin/pages_edit.html")]
#[allow(dead_code)] // flash + flash_kind reserved for flash messaging
struct EditTpl {
    csrf: String,
    flash: Option<String>,
    flash_kind: String,
    id: i64,
    title: String,
    slug: String,
    body_md: String,
    body_html: String,
    status: String,
}

pub async fn handler(
    State(state): State<AppState>,
    Extension(session): Extension<SessionCtx>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    let page = db::pages::find_by_id(&state.pool, id).await.map_err(|e| {
        if matches!(e, db::DbError::NotFound) {
            AppError::NotFound
        } else {
            AppError::from(e)
        }
    })?;

    Ok(EditTpl {
        csrf: session.csrf_token.clone(),
        flash: None,
        flash_kind: String::new(),
        id: page.id,
        title: page.title,
        slug: page.slug,
        body_md: page.body_md,
        body_html: page.body_html,
        status: page.status,
    })
}
