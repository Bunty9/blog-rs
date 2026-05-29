//! Admin dashboard routes. All routes (except /admin/login + /admin/logout from
//! Plan 1b) require an authenticated admin session and CSRF validation on
//! mutating verbs.

pub mod dashboard;
pub mod login;
pub mod logout;
pub mod members_list;
pub mod posts_delete;
pub mod posts_edit;
pub mod posts_list;
pub mod posts_new;
pub mod posts_preview;
pub mod posts_publish;
pub mod posts_save;
pub mod settings;

use crate::middleware::{auth_required, csrf};
use crate::state::AppState;
use axum::routing::{get, post};
use axum::Router;

/// Build the admin sub-router.
///
/// Login / logout (from Plan 1b) remain mounted via their own routers so login
/// stays reachable without a session. The authenticated routes below sit
/// behind csrf + auth_required layers (stacked bottom-up: auth_required runs
/// first, then csrf validates against the loaded session).
pub fn router(state: AppState) -> Router<AppState> {
    let protected = Router::new()
        .route("/", get(dashboard::handler))
        .route("/posts", get(posts_list::handler))
        .route("/posts/new", get(posts_new::get).post(posts_new::post))
        .route(
            "/posts/:id",
            get(posts_edit::handler).post(posts_save::handler),
        )
        .route("/posts/:id/publish", post(posts_publish::handler))
        .route("/posts/:id/delete", post(posts_delete::handler))
        .route("/posts/:id/preview", post(posts_preview::handler))
        .route("/members", get(members_list::handler))
        .route("/members/export.csv", get(members_list::export_csv))
        .route("/settings", get(settings::get).post(settings::post))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            csrf::layer,
        ))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            auth_required::layer,
        ));

    Router::new()
        .merge(login::router())
        .merge(logout::router(state))
        .merge(protected)
}
