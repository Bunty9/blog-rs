//! POST /admin/posts/:id/publish — flip status to published and fan out
//! the newsletter outbox in one transaction.
//!
//! Auth + CSRF are enforced by the surrounding middleware. Returns a small
//! flash partial so htmx swaps can replace the action bar with a confirmation.

use askama::Template;
use askama_axum::IntoResponse;
use axum::extract::{Path, State};

use crate::error::AppError;
use crate::state::AppState;
use db::posts;

#[derive(Template)]
#[template(path = "admin/partials/flash.html")]
struct FlashTpl {
    flash: Option<String>,
    flash_kind: String,
}

pub async fn handler(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    // Always snapshot right before publish, regardless of the save-throttle
    // policy — this is the one revision an author can always get back to.
    let current = posts::find_by_id(&state.pool, id).await?;
    db::revisions::snapshot(
        &state.pool,
        id,
        &current.title,
        current.subtitle.as_deref(),
        &current.body_md,
        current.meta_json.as_deref(),
    )
    .await?;

    let enqueued = posts::publish(&state.pool, id).await?;
    Ok(FlashTpl {
        flash: Some(format!(
            "Published — queued {enqueued} newsletter dispatches."
        )),
        flash_kind: "ok".into(),
    })
}
