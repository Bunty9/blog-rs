//! GET /admin/posts/new — render a minimal new-draft form.
//! POST /admin/posts/new — create a blank draft and redirect to its editor.
//!
//! CSRF is enforced by the surrounding middleware; we read the session ctx
//! out of the request extensions.

use askama::Template;
use askama_axum::IntoResponse;
use axum::extract::State;
use axum::response::Redirect;
use axum::Extension;

use crate::error::AppError;
use crate::middleware::auth_required::SessionCtx;
use crate::state::AppState;
use db::posts::{self, NewPost};

#[derive(Template)]
#[template(path = "admin/posts_new.html")]
struct NewTpl {
    csrf: String,
    flash: Option<String>,
    flash_kind: String,
}

pub async fn get(Extension(session): Extension<SessionCtx>) -> Result<impl IntoResponse, AppError> {
    Ok(NewTpl {
        csrf: session.csrf_token.clone(),
        flash: None,
        flash_kind: String::new(),
    })
}

pub async fn post(
    State(state): State<AppState>,
    Extension(session): Extension<SessionCtx>,
) -> Result<Redirect, AppError> {
    let now = time::OffsetDateTime::now_utc().unix_timestamp();
    let slug = format!("draft-{now}");
    let id = posts::create(
        &state.pool,
        NewPost {
            slug: &slug,
            title: "Untitled",
            subtitle: None,
            status: "draft",
            author_id: session.user_id,
            excerpt: None,
            cover_image: None,
            body_md: "",
            body_html: "",
            meta_json: None,
        },
    )
    .await?;
    Ok(Redirect::to(&format!("/admin/posts/{id}")))
}
