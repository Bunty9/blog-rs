//! GET /admin/posts — list view with status filter + LIKE search.
//!
//! Auth + CSRF live in the surrounding middleware. The handler only consumes
//! the `SessionCtx` extension and renders.

use askama::Template;
use askama_axum::IntoResponse;
use axum::extract::{Query, State};
use axum::Extension;
use serde::Deserialize;

use crate::error::AppError;
use crate::middleware::auth_required::SessionCtx;
use crate::state::AppState;
use db::posts::{self, AdminPostRow, PostStatusFilter};

#[derive(Deserialize, Default)]
pub struct ListQuery {
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub q: String,
}

#[derive(Template)]
#[template(path = "admin/posts_list.html")]
struct PostsListTpl {
    csrf: String,
    flash: Option<String>,
    flash_kind: String,
    status: String,
    q: String,
    rows: Vec<AdminPostRow>,
}

pub async fn handler(
    State(state): State<AppState>,
    Extension(session): Extension<SessionCtx>,
    Query(q): Query<ListQuery>,
) -> Result<impl IntoResponse, AppError> {
    let filter = PostStatusFilter::parse(&q.status);
    let search = if q.q.trim().is_empty() {
        None
    } else {
        Some(q.q.trim())
    };
    let rows = posts::list_admin(&state.pool, filter, search, 200).await?;
    Ok(PostsListTpl {
        csrf: session.csrf_token.clone(),
        flash: None,
        flash_kind: String::new(),
        status: filter.to_string(),
        q: q.q,
        rows,
    })
}
