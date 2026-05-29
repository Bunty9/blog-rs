//! Filled in by a later task in plan 1b.
use axum::Router;
use crate::state::AppState;
pub fn router() -> Router<AppState> { Router::new() }
