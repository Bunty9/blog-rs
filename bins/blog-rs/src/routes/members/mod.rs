//! Public member-facing routes: signup, double-opt-in confirm, unsubscribe,
//! and the preferences endpoint. None of these routes require an admin
//! session — authorization for confirm/unsubscribe/preferences comes from
//! HMAC-signed tokens (see `crate::tokens`). Signup is open to anyone.

use axum::routing::{get, post};
use axum::Router;

use crate::state::AppState;

pub mod confirm;
pub mod preferences;
pub mod signup;
pub mod unsubscribe;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/signup", get(signup::show).post(signup::submit))
        .route("/confirm/:token", get(confirm::confirm))
        .route(
            "/unsubscribe/:token",
            get(unsubscribe::show).post(unsubscribe::submit),
        )
        .route("/preferences", post(preferences::submit))
}
