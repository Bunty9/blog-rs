//! POST /admin/pages/:id/delete — permanently delete a static page and redirect
//! to the list.
//!
//! Auth + CSRF are enforced by the surrounding middleware.

use axum::extract::{Path, State};
use axum::response::Redirect;

use crate::error::AppError;
use crate::state::AppState;
use db::pages;

pub async fn handler(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Redirect, AppError> {
    pages::delete(&state.pool, id).await?;
    Ok(Redirect::to("/admin/pages"))
}
