//! GET /posts/:slug — render a single published post with per-block asset
//! manifest injection.

use askama::Template;
use askama_axum::IntoResponse;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use db::posts::Post;

use crate::error::AppError;
use crate::state::AppState;
use crate::view::{iso_date, AssetTag, SiteCtx};

#[derive(Debug, Clone)]
pub struct PostView {
    pub slug: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub cover_image: Option<String>,
    pub body_html: String,
    pub published_date: Option<String>,
    pub reading_minutes: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct TagLink {
    pub slug: String,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct SeriesLink {
    pub slug: String,
}

#[derive(Template)]
#[template(path = "reader/post.html")]
pub struct PostTemplate {
    pub site: SiteCtx,
    pub asset_tags: Vec<AssetTag>,
    pub post: PostView,
    pub tags: Vec<TagLink>,
    pub series: Option<SeriesLink>,
}

pub async fn handler(
    Path(slug): Path<String>,
    State(state): State<AppState>,
) -> Result<axum::response::Response, AppError> {
    let post: Post = match db::posts::find_by_slug(&state.pool, &slug).await {
        Ok(p) if p.status == "published" => p,
        Ok(_) => {
            return Ok((StatusCode::NOT_FOUND, "404 — post not found").into_response());
        }
        Err(db::DbError::NotFound) => {
            return Ok((StatusCode::NOT_FOUND, "404 — post not found").into_response());
        }
        Err(e) => return Err(AppError::from(e)),
    };

    let tag_rows = db::tags::list_for_post(&state.pool, post.id).await?;
    let tags: Vec<TagLink> = tag_rows
        .into_iter()
        .map(|t| TagLink {
            slug: t.slug,
            name: t.name,
        })
        .collect();

    let series = post
        .meta_json
        .as_deref()
        .and_then(|m| serde_json::from_str::<serde_json::Value>(m).ok())
        .and_then(|v| {
            v.get("series")
                .and_then(|s| s.as_str())
                .map(|s| s.to_string())
        })
        .map(|slug| SeriesLink { slug });

    let asset_tags = AssetTag::from_manifest(&post.assets());

    let view = PostView {
        slug: post.slug.clone(),
        title: post.title.clone(),
        subtitle: post.subtitle.clone(),
        cover_image: post.cover_image.clone(),
        body_html: post.body_html.clone(),
        published_date: post.published_at.map(iso_date),
        reading_minutes: post.reading_minutes,
    };

    Ok(PostTemplate {
        site: SiteCtx::placeholder(),
        asset_tags,
        post: view,
        tags,
        series,
    }
    .into_response())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use axum::body::Body;
    use axum::http::Request;
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

    #[tokio::test]
    async fn renders_post_with_body_html_and_assets() {
        let (app, pool) = test_app().await;
        let assets = r#"{"assets":[{"kind":"Css","src":"/assets/blocks/callout.css","defer":false}]}"#;
        sqlx::query(
            r#"
            INSERT INTO posts (slug, title, subtitle, status, author_id, published_at,
                               updated_at, created_at, body_md, body_html, meta_json, assets_json)
            VALUES ('boot-up', 'Booting a Cortex-M4', 'no_std notes', 'published', 1,
                    1700000000, 1700000000, 1700000000,
                    '# x', '<aside class="callout callout-info">heads up</aside>', '{}', ?)
            "#,
        )
        .bind(assets)
        .execute(&pool)
        .await
        .unwrap();

        let res = app
            .oneshot(
                Request::builder()
                    .uri("/posts/boot-up")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let body = std::str::from_utf8(&bytes).unwrap();
        assert!(body.contains("Booting a Cortex-M4"));
        assert!(body.contains("no_std notes"));
        assert!(body.contains(r#"class="callout callout-info""#));
        assert!(body.contains(r#"href="/assets/blocks/callout.css""#));
    }

    #[tokio::test]
    async fn unknown_slug_returns_404() {
        let (app, _pool) = test_app().await;
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/posts/does-not-exist")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
    }
}
