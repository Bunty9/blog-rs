//! GET /admin/pages/new — render new page form.
//! POST /admin/pages/new — create a blank draft page and redirect to its editor.
//!
//! CSRF is enforced by the surrounding middleware.

use askama::Template;
use askama_axum::IntoResponse;
use axum::extract::State;
use axum::response::Redirect;
use axum::Extension;

use crate::error::AppError;
use crate::middleware::auth_required::SessionCtx;
use crate::state::AppState;
use db::pages::{self, NewPage};

#[derive(Template)]
#[template(path = "admin/pages_new.html")]
#[allow(dead_code)] // flash + flash_kind reserved for flash messaging
struct NewTpl {
    csrf: String,
    nav: &'static str,
    page_title: &'static str,
    flash: Option<String>,
    flash_kind: String,
}

pub async fn get(Extension(session): Extension<SessionCtx>) -> Result<impl IntoResponse, AppError> {
    Ok(NewTpl {
        csrf: session.csrf_token.clone(),
        nav: "pages",
        page_title: "New page",
        flash: None,
        flash_kind: String::new(),
    })
}

pub async fn post(
    State(state): State<AppState>,
) -> Result<Redirect, AppError> {
    let now = time::OffsetDateTime::now_utc().unix_timestamp();
    let slug = format!("draft-page-{now}");
    let id = pages::create(
        &state.pool,
        NewPage {
            slug: &slug,
            title: "Untitled Page",
            body_md: "",
            body_html: "",
            toc_json: "[]",
            meta_json: None,
            status: "draft",
        },
    )
    .await?;
    Ok(Redirect::to(&format!("/admin/pages/{id}/edit")))
}
