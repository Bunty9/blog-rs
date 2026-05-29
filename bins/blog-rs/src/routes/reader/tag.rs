use crate::state::AppState;
use axum::{
    extract::{Path, State},
    response::IntoResponse,
};

pub async fn handler(
    Path(_slug): Path<String>,
    State(_state): State<AppState>,
) -> impl IntoResponse {
    "tag stub"
}
