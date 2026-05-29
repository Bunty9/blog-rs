pub mod login;
pub mod logout;

use axum::Router;
use crate::state::AppState;

pub fn router() -> Router<AppState> { Router::new() }
