//! GET /feed.xml — RSS 2.0 feed of the most recent published posts.

use crate::error::AppError;
use crate::state::AppState;
use crate::view::{rfc2822, SiteCtx};
use askama::Template;
use axum::extract::State;
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use db::posts;

#[derive(Debug, Clone)]
pub struct FeedItem {
    pub slug: String,
    pub title: String,
    pub pub_date: String,
    pub description: Option<String>,
}

#[derive(Template)]
#[template(path = "xml/feed.xml", escape = "xml")]
pub struct FeedTemplate {
    pub site: SiteCtx,
    pub items: Vec<FeedItem>,
}

const FEED_LIMIT: i64 = 20;

pub async fn handler(State(state): State<AppState>) -> Result<Response, AppError> {
    let rows = posts::list_published(&state.pool, FEED_LIMIT, 0).await?;
    let items: Vec<FeedItem> = rows
        .iter()
        .map(|p| FeedItem {
            slug: p.slug.clone(),
            title: p.title.clone(),
            pub_date: p.published_at.map(rfc2822).unwrap_or_default(),
            description: p.excerpt.clone(),
        })
        .collect();

    let tpl = FeedTemplate {
        site: SiteCtx::placeholder(),
        items,
    };
    let body = tpl
        .render()
        .map_err(|e| AppError::Internal(format!("feed render: {e}")))?;

    let mut res = (StatusCode::OK, body).into_response();
    res.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/rss+xml; charset=utf-8"),
    );
    Ok(res)
}

#[cfg(test)]
mod tests {
    use crate::state::AppState;
    use axum::body::{to_bytes, Body};
    use axum::http::{Request, StatusCode};
    use db::test_support::fresh_pool;
    use db::SqlitePool;
    use tower::ServiceExt;

    async fn test_app() -> (axum::Router, SqlitePool) {
        let pool = fresh_pool().await;
        sqlx::query(
            "INSERT INTO users (id, email, password_hash, role, created_at) \
             VALUES (1, 'a@b', 'x', 'admin', 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        let cfg = crate::config::Config::default();
        let state = AppState::new(pool.clone(), cfg, vec![0u8; 32]);
        (crate::routes::router(state), pool)
    }

    #[tokio::test]
    async fn feed_serves_rss_content_type_and_one_item() {
        let (app, pool) = test_app().await;
        sqlx::query(
            r#"INSERT INTO posts (slug, title, status, author_id, published_at, updated_at, created_at, body_md, body_html, meta_json, assets_json, excerpt)
               VALUES ('p', 'A & B', 'published', 1, 1700000000, 0, 0, '#x', '<h1>x</h1>', '{}', '[]', 'short')"#,
        )
        .execute(&pool)
        .await
        .unwrap();

        let res = app
            .oneshot(
                Request::builder()
                    .uri("/feed.xml")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        assert!(res
            .headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("application/rss+xml"));
        let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let body = std::str::from_utf8(&bytes).unwrap();
        assert!(body.starts_with("<?xml"));
        assert!(body.contains("<title>A &amp; B</title>"));
        assert!(body.contains("Tue, 14 Nov 2023 22:13:20 +0000"));
        assert!(body.contains("<guid"));
    }

    #[tokio::test]
    async fn feed_empty_db_renders_valid_xml() {
        let (app, _pool) = test_app().await;
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/feed.xml")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let body = std::str::from_utf8(&bytes).unwrap();
        assert!(body.contains("<rss"));
        assert!(body.contains("</rss>"));
    }
}
