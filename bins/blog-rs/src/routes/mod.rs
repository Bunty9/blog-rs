pub mod admin;
pub mod health;

use axum::Router;
use tower::ServiceBuilder;
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::compression::CompressionLayer;
use tower_http::trace::TraceLayer;

use crate::middleware::correlation_id;
use crate::state::AppState;

pub fn router(state: AppState) -> Router {
    let service = ServiceBuilder::new()
        .layer(CatchPanicLayer::new())
        .layer(TraceLayer::new_for_http())
        .layer(CompressionLayer::new())
        .layer(axum::middleware::from_fn(correlation_id::layer));

    Router::new()
        .merge(health::router())
        .nest("/admin", admin::router())
        .route("/assets/*path", axum::routing::get(crate::embed::handler))
        .layer(service)
        .with_state(state)
}
