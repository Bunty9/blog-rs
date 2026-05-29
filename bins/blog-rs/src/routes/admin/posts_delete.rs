//! POST /admin/posts/:id/delete — soft-delete the post and bounce to the list.
//!
//! Auth + CSRF are enforced by the surrounding middleware. We use a 303
//! redirect so browsers replay as GET on the list page; htmx clients can
//! follow the Location header or treat the response as terminal.

use axum::extract::{Path, State};
use axum::response::Redirect;

use crate::error::AppError;
use crate::state::AppState;
use db::posts;

pub async fn handler(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Redirect, AppError> {
    posts::soft_delete(&state.pool, id).await?;
    Ok(Redirect::to("/admin/posts"))
}
