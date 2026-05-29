use crate::state::AppState;
use crate::view::{iso_date, SiteCtx};
use askama::Template;
use axum::extract::State;
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};

#[derive(Debug, Clone)]
pub struct SitemapEntry {
    pub slug: String,
    pub lastmod: Option<String>,
}

#[derive(Template)]
#[template(path = "reader/sitemap.xml", escape = "html")]
pub struct SitemapTemplate {
    pub site: SiteCtx,
    pub posts: Vec<SitemapEntry>,
    pub tags: Vec<String>,
    pub series_list: Vec<String>,
}

pub async fn handler(State(state): State<AppState>) -> Result<Response, crate::error::AppError> {
    let post_rows = db::posts::list_published(&state.pool, 1000, 0).await?;
    let posts: Vec<SitemapEntry> = post_rows
        .iter()
        .map(|p| SitemapEntry {
            slug: urlencoding::encode(&p.slug).to_string(),
            lastmod: Some(iso_date(p.updated_at)),
        })
        .collect();

    // `db::tags::list_all` doesn't exist yet — inline the scalar query.
    let tag_slugs: Vec<(String,)> = sqlx::query_as("SELECT slug FROM tags ORDER BY slug ASC")
        .fetch_all(&state.pool)
        .await
        .map_err(db::DbError::from)?;
    let tags: Vec<String> = tag_slugs
        .into_iter()
        .map(|(s,)| urlencoding::encode(&s).to_string())
        .collect();

    let series_rows = db::posts::list_series_slugs(&state.pool)
        .await
        .map_err(db::DbError::from)?;
    let series_list: Vec<String> = series_rows
        .into_iter()
        .map(|s| urlencoding::encode(&s).to_string())
        .collect();

    let tpl = SitemapTemplate {
        site: SiteCtx::placeholder(),
        posts,
        tags,
        series_list,
    };
    let body = tpl
        .render()
        .map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    let mut res = (StatusCode::OK, body).into_response();
    res.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/xml; charset=utf-8"),
    );
    Ok(res)
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
        (crate::routes::router(state), pool)
    }

    #[tokio::test]
    async fn sitemap_lists_posts_and_tags() {
        let (app, pool) = test_app().await;
        sqlx::query("INSERT INTO tags (slug, name) VALUES ('rust', 'Rust')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            r#"INSERT INTO posts (slug, title, status, author_id, published_at, updated_at, created_at, body_md, body_html, meta_json, assets_json)
               VALUES ('hello', 'H', 'published', 1, 1700000000, 1700000000, 1700000000, '#x', '<h1>x</h1>', '{"series":"rust-level-4"}', '[]')"#,
        )
        .execute(&pool)
        .await
        .unwrap();

        let res = app
            .oneshot(
                Request::builder()
                    .uri("/sitemap.xml")
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
            .contains("application/xml"));
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let body = std::str::from_utf8(&bytes).unwrap();
        assert!(body.contains("/posts/hello"));
        assert!(body.contains("/tags/rust"));
        assert!(body.contains("/series/rust-level-4"));
        assert!(body.contains("2023-11-14"));
    }

    #[tokio::test]
    async fn sitemap_escapes_slug_with_special_chars() {
        let (app, pool) = test_app().await;
        sqlx::query(
            r#"INSERT INTO posts (slug, title, status, author_id, published_at, updated_at, created_at, body_md, body_html, meta_json, assets_json)
               VALUES ('weird slug', 'W', 'published', 1, 1700000000, 1700000000, 1700000000, '#x', '<h1>x</h1>', '{}', '[]')"#,
        )
        .execute(&pool)
        .await
        .unwrap();
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/sitemap.xml")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let body = std::str::from_utf8(&bytes).unwrap();
        // `urlencoding` turns ' ' into '%20'
        assert!(body.contains("/posts/weird%20slug"));
    }
}
