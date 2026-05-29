pub mod login;
pub mod logout;

use crate::state::AppState;
use axum::Router;

pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .merge(login::router())
        .merge(logout::router(state))
}
