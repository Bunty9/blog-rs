use crate::state::AppState;
use crate::view::{clamp_page, iso_date, AssetTag, Pagination, SiteCtx, PAGE_SIZE};
use askama::Template;
use askama_axum::IntoResponse;
use axum::extract::{Query, State};
use db::posts;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct HomeQuery {
    pub page: Option<i64>,
}

/// View-model for one card. The handler precomputes the date strings so the
/// template never reaches for the chrono crate.
#[derive(Debug, Clone)]
pub struct PostCard {
    pub slug: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub excerpt: Option<String>,
    pub published_date: Option<String>,
    pub reading_minutes: Option<i64>,
}

impl From<&posts::Post> for PostCard {
    fn from(p: &posts::Post) -> Self {
        Self {
            slug: p.slug.clone(),
            title: p.title.clone(),
            subtitle: p.subtitle.clone(),
            excerpt: p.excerpt.clone(),
            published_date: p.published_at.map(iso_date),
            reading_minutes: p.reading_minutes,
        }
    }
}

#[derive(Template)]
#[template(path = "reader/home.html")]
pub struct HomeTemplate {
    pub site: SiteCtx,
    pub asset_tags: Vec<AssetTag>,
    pub nav: &'static str,
    pub cards: Vec<PostCard>,
    pub pagination: Pagination,
}

pub async fn handler(
    State(state): State<AppState>,
    Query(q): Query<HomeQuery>,
) -> Result<impl IntoResponse, crate::error::AppError> {
    // `count_published` doesn't exist in db::posts yet; inline the scalar query.
    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM posts WHERE status = 'published'")
        .fetch_one(&state.pool)
        .await
        .map_err(db::DbError::from)?;
    let page = clamp_page(q.page, total);
    let offset = (page - 1) * PAGE_SIZE;
    let rows = posts::list_published(&state.pool, PAGE_SIZE, offset).await?;
    let cards: Vec<PostCard> = rows.iter().map(PostCard::from).collect();

    Ok(HomeTemplate {
        site: SiteCtx::placeholder(),
        asset_tags: Vec::new(), // home page has no shortcode assets
        nav: "home",
        cards,
        pagination: Pagination::new(page, total, "/"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use axum::body::{to_bytes, Body};
    use axum::http::{Request, StatusCode};
    use sqlx::SqlitePool;
    use tower::ServiceExt;

    async fn seed(pool: &SqlitePool, slug: &str, title: &str, when: i64) {
        sqlx::query(
            r#"
            INSERT OR IGNORE INTO users (id, email, password_hash, role, created_at)
            VALUES (1, 'a@b', 'x', 'admin', 0)
            "#,
        )
        .execute(pool)
        .await
        .unwrap();

        sqlx::query(
            r#"
            INSERT INTO posts (slug, title, status, author_id, published_at,
                               updated_at, created_at, excerpt, reading_minutes,
                               body_md, body_html, meta_json, assets_json)
            VALUES (?, ?, 'published', 1, ?, ?, ?, 'short excerpt', 3,
                    '# x', '<h1>x</h1>', '{}', '[]')
            "#,
        )
        .bind(slug)
        .bind(title)
        .bind(when)
        .bind(when)
        .bind(when)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn test_app() -> (axum::Router, SqlitePool) {
        let pool = db::test_support::fresh_pool().await;
        let state = AppState::new(pool.clone(), Config::default(), vec![0u8; 32]);
        let app = crate::routes::router(state);
        (app, pool)
    }

    #[tokio::test]
    async fn home_renders_card_for_published_post() {
        let (app, pool) = test_app().await;
        seed(&pool, "hello-world", "Hello World", 1_700_000_000).await;

        let res = app
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let body = std::str::from_utf8(&bytes).unwrap();
        assert!(body.contains("Hello World"), "title missing: {body}");
        assert!(
            body.contains("/posts/hello-world"),
            "slug link missing: {body}"
        );
        assert!(body.contains("2023-11-14"), "iso date missing: {body}");
        assert!(
            body.contains("class=\"post-card\""),
            "post-card class missing: {body}"
        );
    }

    #[tokio::test]
    async fn home_empty_db_returns_200_with_empty_marker() {
        let (app, _pool) = test_app().await;
        let res = app
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let body = std::str::from_utf8(&bytes).unwrap();
        assert!(
            body.contains("No posts yet."),
            "empty marker missing: {body}"
        );
    }
}
