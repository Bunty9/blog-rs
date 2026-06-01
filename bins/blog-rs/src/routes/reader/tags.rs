use crate::state::AppState;
use crate::view::{AssetTag, SiteCtx};
use askama::Template;
use askama_axum::IntoResponse;
use axum::extract::State;
use db::tags::TagCount;

#[derive(Template)]
#[template(path = "reader/tags.html")]
pub struct TagsTemplate {
    pub site: SiteCtx,
    pub asset_tags: Vec<AssetTag>,
    pub nav: &'static str,
    pub tags: Vec<TagCount>,
}

pub async fn handler(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, crate::error::AppError> {
    let tags = db::tags::list_with_counts(&state.pool).await?;
    Ok(TagsTemplate {
        site: SiteCtx::placeholder(),
        asset_tags: Vec::new(),
        nav: "tags",
        tags,
    })
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

    async fn seed_tag(pool: &SqlitePool, slug: &str, name: &str) {
        sqlx::query("INSERT OR IGNORE INTO tags (slug, name) VALUES (?, ?)")
            .bind(slug)
            .bind(name)
            .execute(pool)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn tags_index_returns_200_with_seeded_tag() {
        let (app, pool) = test_app().await;
        seed_tag(&pool, "rust", "Rust").await;

        let res = app
            .oneshot(Request::builder().uri("/tags").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let body = std::str::from_utf8(&bytes).unwrap();
        assert!(body.contains("Rust"), "tag name missing: {body}");
    }

    #[tokio::test]
    async fn tags_index_empty_db_returns_200() {
        let (app, _pool) = test_app().await;
        let res = app
            .oneshot(Request::builder().uri("/tags").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let body = std::str::from_utf8(&bytes).unwrap();
        assert!(
            body.contains("No tags yet."),
            "empty marker missing: {body}"
        );
    }
}
