//! Public-facing reader surface (spec §6.1).

use crate::state::AppState;
use axum::{routing::get, Router};

pub mod feed;
pub mod home;
pub mod post;
pub mod robots;
pub mod search;
pub mod series;
pub mod sitemap;
pub mod tag;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(home::handler))
        .route("/posts/:slug", get(post::handler))
        .route("/tags/:slug", get(tag::handler))
        .route("/series/:slug", get(series::handler))
        .route("/search", get(search::handler))
        .route("/feed.xml", get(feed::handler))
        .route("/sitemap.xml", get(sitemap::handler))
        .route("/robots.txt", get(robots::handler))
}
