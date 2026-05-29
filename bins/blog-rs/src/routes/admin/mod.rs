pub mod login;
pub mod logout;

use axum::Router;
use crate::state::AppState;

pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .merge(login::router())
        .merge(logout::router(state))
}
