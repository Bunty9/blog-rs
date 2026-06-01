//! Public-facing reader surface (spec §6.1).

use crate::state::AppState;
use axum::{routing::get, Router};

pub mod error;
pub mod feed;
pub mod home;
pub mod post;
pub mod robots;
pub mod search;
pub mod series;
pub mod sitemap;
pub mod tag;
pub mod tags;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(home::handler))
        .route("/posts/:slug", get(post::handler))
        .route("/tags", get(tags::handler))
        .route("/tags/:slug", get(tag::handler))
        .route("/series/:slug", get(series::handler))
        .route("/search", get(search::handler))
        .route("/search/instant", get(search::instant_handler))
        .route("/feed.xml", get(feed::handler))
        .route("/sitemap.xml", get(sitemap::handler))
        .route("/robots.txt", get(robots::handler))
}
