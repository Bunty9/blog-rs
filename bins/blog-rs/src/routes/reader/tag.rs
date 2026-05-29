use crate::routes::reader::home::PostCard;
use crate::state::AppState;
use crate::view::{clamp_page, AssetTag, Pagination, SiteCtx, PAGE_SIZE};
use askama::Template;
use askama_axum::IntoResponse;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct TagQuery {
    pub page: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct TagView {
    pub slug: String,
    pub name: String,
}

#[derive(Template)]
#[template(path = "reader/tag.html")]
pub struct TagTemplate {
    pub site: SiteCtx,
    pub asset_tags: Vec<AssetTag>,
    pub tag: TagView,
    pub cards: Vec<PostCard>,
    pub pagination: Pagination,
}

pub async fn handler(
    Path(slug): Path<String>,
    Query(q): Query<TagQuery>,
    State(state): State<AppState>,
) -> Result<axum::response::Response, crate::error::AppError> {
    let tag = match db::tags::find_by_slug(&state.pool, &slug).await {
        Ok(t) => t,
        Err(db::DbError::NotFound) => {
            return Ok((StatusCode::NOT_FOUND, "404 — tag not found").into_response());
        }
        Err(e) => return Err(e.into()),
    };

    let total = db::posts::count_by_tag(&state.pool, &slug).await?;
    let page = clamp_page(q.page, total);
    let offset = (page - 1) * PAGE_SIZE;
    let rows = db::posts::list_by_tag(&state.pool, &slug, PAGE_SIZE, offset).await?;
    let cards: Vec<PostCard> = rows.iter().map(PostCard::from).collect();

    Ok(TagTemplate {
        site: SiteCtx::placeholder(),
        asset_tags: Vec::new(),
        tag: TagView {
            slug: tag.slug,
            name: tag.name,
        },
        cards,
        pagination: Pagination::new(page, total, format!("/tags/{}", slug)),
    }
    .into_response())
}

#[cfg(test)]
mod tests {
    use crate::config::Config;
    use crate::state::AppState;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use sqlx::SqlitePool;
    use tower::ServiceExt;

    async fn test_app() -> (axum::Router, SqlitePool) {
        let pool = db::test_support::fresh_pool().await;
        sqlx::query(
            "INSERT OR IGNORE INTO users (id, email, password_hash, role, created_at) \
             VALUES (1, 'a@b', 'x', 'admin', 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        let state = AppState::new(pool.clone(), Config::default(), vec![0u8; 32]);
        let app = crate::routes::router(state);
        (app, pool)
    }

    async fn tag_post(
        pool: &SqlitePool,
        tag_slug: &str,
        tag_name: &str,
        post_slug: &str,
        post_title: &str,
    ) {
        sqlx::query("INSERT OR IGNORE INTO tags (slug, name) VALUES (?, ?)")
            .bind(tag_slug)
            .bind(tag_name)
            .execute(pool)
            .await
            .unwrap();
        sqlx::query(
            r#"INSERT INTO posts (slug, title, status, author_id, published_at, updated_at, created_at, body_md, body_html, meta_json, assets_json)
               VALUES (?, ?, 'published', 1, 1700000000, 1700000000, 1700000000, '#x', '<h1>x</h1>', '{}', '[]')"#,
        )
        .bind(post_slug)
        .bind(post_title)
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO post_tags (post_id, tag_id) \
             SELECT p.id, t.id FROM posts p, tags t WHERE p.slug = ? AND t.slug = ?",
        )
        .bind(post_slug)
        .bind(tag_slug)
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn tag_page_lists_only_tagged_posts() {
        let (app, pool) = test_app().await;
        tag_post(&pool, "rust", "Rust", "p1", "Post 1").await;
        tag_post(&pool, "go", "Go", "p2", "Post 2").await;

        let res = app
            .oneshot(
                Request::builder()
                    .uri("/tags/rust")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let body = std::str::from_utf8(&bytes).unwrap();
        assert!(body.contains("Post 1"), "Post 1 missing: {body}");
        assert!(!body.contains("Post 2"), "Post 2 should not appear: {body}");
        assert!(body.contains("#Rust"), "tag name missing: {body}");
    }

    #[tokio::test]
    async fn unknown_tag_404() {
        let (app, _pool) = test_app().await;
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/tags/nope")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn known_tag_with_no_posts_returns_200_empty() {
        let (app, pool) = test_app().await;
        sqlx::query("INSERT INTO tags (slug, name) VALUES ('empty', 'Empty')")
            .execute(&pool)
            .await
            .unwrap();
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/tags/empty")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let body = std::str::from_utf8(&bytes).unwrap();
        assert!(
            body.contains("No posts under this tag yet."),
            "empty marker missing: {body}"
        );
    }
}
