use crate::state::AppState;
use axum::{extract::State, response::IntoResponse};

pub async fn handler(State(_state): State<AppState>) -> impl IntoResponse {
    "home stub"
}
