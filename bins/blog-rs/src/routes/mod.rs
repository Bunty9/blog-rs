pub mod admin;
pub mod health;

use axum::Router;

use crate::state::AppState;

pub fn router(state: AppState) -> Router {
    Router::new()
        .merge(health::router())
        .nest("/admin", admin::router())
        .route("/assets/*path", axum::routing::get(crate::embed::handler))
        .with_state(state)
}
