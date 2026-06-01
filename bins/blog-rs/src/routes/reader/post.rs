//! GET /posts/:slug — render a single published post with per-block asset
//! manifest injection.

use askama::Template;
use askama_axum::IntoResponse;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use db::posts::Post;

use crate::error::AppError;
use crate::routes::reader::home::PostCard;
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
    pub toc: Vec<content::TocEntry>,
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
    pub nav: &'static str,
    pub post: PostView,
    pub tags: Vec<TagLink>,
    pub series: Option<SeriesLink>,
    pub related: Vec<PostCard>,
    pub prev: Option<PostCard>,
    pub next: Option<PostCard>,
}

pub async fn handler(
    Path(slug): Path<String>,
    State(state): State<AppState>,
) -> Result<axum::response::Response, AppError> {
    let mut post: Post = match db::posts::find_by_slug(&state.pool, &slug).await {
        Ok(p) if p.status == "published" => p,
        Ok(_) => {
            return Ok((StatusCode::NOT_FOUND, "404 — post not found").into_response());
        }
        Err(db::DbError::NotFound) => {
            return Ok((StatusCode::NOT_FOUND, "404 — post not found").into_response());
        }
        Err(e) => return Err(AppError::from(e)),
    };

    // Lazy regen: if the cached HTML was rendered by an older RENDER_VERSION
    // (e.g. legacy rows where the column defaults to 0), re-render from body_md
    // and persist before serving. See spec §5.2.
    let cached_version = db::posts::body_html_version(&state.pool, post.id).await?;
    if cached_version < content::RENDER_VERSION as i64 {
        let out = content::render(&post.body_md)
            .map_err(|e| AppError::Internal(format!("re-render failed: {e}")))?;
        let assets_json = serde_json::to_string(&out.assets).unwrap_or_else(|_| "[]".into());
        let toc_json = serde_json::to_string(&out.toc).unwrap_or_else(|_| "[]".into());
        db::posts::update_rendered_cache(
            &state.pool,
            post.id,
            &out.html,
            &assets_json,
            &toc_json,
            out.reading_minutes,
            content::RENDER_VERSION as i64,
        )
        .await?;
        post.body_html = out.html;
        post.assets_json = assets_json;
        post.toc_json = toc_json;
        post.reading_minutes = Some(out.reading_minutes);
    }

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

    let toc: Vec<content::TocEntry> =
        serde_json::from_str::<Vec<content::TocEntry>>(&post.toc_json).unwrap_or_default();

    let related_posts = db::posts::related(&state.pool, post.id, 3).await?;
    let related: Vec<PostCard> = related_posts.iter().map(PostCard::from).collect();

    let (prev_post, next_post) = db::posts::neighbors(&state.pool, post.id).await?;
    let prev: Option<PostCard> = prev_post.as_ref().map(PostCard::from);
    let next: Option<PostCard> = next_post.as_ref().map(PostCard::from);

    let view = PostView {
        slug: post.slug.clone(),
        title: post.title.clone(),
        subtitle: post.subtitle.clone(),
        cover_image: post.cover_image.clone(),
        body_html: post.body_html.clone(),
        published_date: post.published_at.map(iso_date),
        reading_minutes: post.reading_minutes,
        toc,
    };

    Ok(PostTemplate {
        site: SiteCtx::placeholder(),
        asset_tags,
        nav: "",
        post: view,
        tags,
        series,
        related,
        prev,
        next,
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
        let assets =
            r#"{"assets":[{"kind":"Css","src":"/assets/blocks/callout.css","defer":false}]}"#;
        sqlx::query(
            r#"
            INSERT INTO posts (slug, title, subtitle, status, author_id, published_at,
                               updated_at, created_at, body_md, body_html,
                               body_html_version, meta_json, assets_json)
            VALUES ('boot-up', 'Booting a Cortex-M4', 'no_std notes', 'published', 1,
                    1700000000, 1700000000, 1700000000,
                    '# x', '<aside class="callout callout-info">heads up</aside>', ?, '{}', ?)
            "#,
        )
        .bind(content::RENDER_VERSION as i64)
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
    async fn stale_body_html_version_triggers_lazy_regen() {
        let (app, pool) = test_app().await;
        // Seed a row with version=0 and intentionally stale body_html that does
        // NOT match what content::render would produce from body_md.
        sqlx::query(
            r#"
            INSERT INTO posts (slug, title, status, author_id, published_at,
                               updated_at, created_at, body_md, body_html,
                               body_html_version, meta_json, assets_json)
            VALUES ('legacy', 'Legacy', 'published', 1, 1700000000, 1700000000,
                    1700000000, '# Fresh heading', '<p>STALE</p>', 0, '{}', '[]')
            "#,
        )
        .execute(&pool)
        .await
        .unwrap();

        let res = app
            .oneshot(
                Request::builder()
                    .uri("/posts/legacy")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let body = std::str::from_utf8(&bytes).unwrap();
        // Re-rendered output must reflect body_md, not the stale cache.
        assert!(
            !body.contains("STALE"),
            "stale body_html should have been replaced"
        );
        assert!(body.contains("Fresh heading"));

        // Persisted row should now carry the current RENDER_VERSION.
        let v: i64 = sqlx::query_scalar("SELECT body_html_version FROM posts WHERE slug = ?")
            .bind("legacy")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(v, content::RENDER_VERSION as i64);
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

    #[tokio::test]
    async fn meta_strip_present_in_rendered_page() {
        let (app, pool) = test_app().await;
        sqlx::query(
            r#"
            INSERT INTO posts (slug, title, subtitle, status, author_id, published_at,
                               updated_at, created_at, body_md, body_html,
                               body_html_version, meta_json, assets_json, reading_minutes)
            VALUES ('meta-strip-test', 'Meta Strip Post', 'a subtitle', 'published', 1,
                    1700000000, 1700000000, 1700000000,
                    '# x', '<p>body</p>', ?, '{}', '[]', 5)
            "#,
        )
        .bind(content::RENDER_VERSION as i64)
        .execute(&pool)
        .await
        .unwrap();

        let res = app
            .oneshot(
                Request::builder()
                    .uri("/posts/meta-strip-test")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let body = std::str::from_utf8(&bytes).unwrap();
        assert!(body.contains("class=\"meta-strip\""), "meta-strip div missing");
        assert!(body.contains("5 min read"), "reading minutes missing");
        assert!(body.contains("2023-11-14"), "published date missing");
        assert!(body.contains("a subtitle"), "subtitle missing");
    }

    #[tokio::test]
    async fn toc_aside_rendered_when_headings_present() {
        let (app, pool) = test_app().await;
        // Pre-compute a real toc_json from markdown with headings
        let md = "## Introduction\n\nSome text.\n\n## Conclusion\n\nDone.";
        let out = content::render(md).expect("render failed");
        let toc_json = serde_json::to_string(&out.toc).unwrap();

        sqlx::query(
            r#"
            INSERT INTO posts (slug, title, status, author_id, published_at,
                               updated_at, created_at, body_md, body_html,
                               body_html_version, meta_json, assets_json, toc_json, reading_minutes)
            VALUES ('toc-post', 'TOC Post', 'published', 1,
                    1700000000, 1700000000, 1700000000,
                    ?, ?, ?, '{}', '[]', ?, ?)
            "#,
        )
        .bind(md)
        .bind(&out.html)
        .bind(content::RENDER_VERSION as i64)
        .bind(&toc_json)
        .bind(out.reading_minutes)
        .execute(&pool)
        .await
        .unwrap();

        let res = app
            .oneshot(
                Request::builder()
                    .uri("/posts/toc-post")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let body = std::str::from_utf8(&bytes).unwrap();
        assert!(
            body.contains("class=\"toc\""),
            "toc nav missing: {body}"
        );
        assert!(
            body.contains("href=\"#introduction\"") || body.contains("href=\"#"),
            "toc anchor link missing"
        );
    }

    #[tokio::test]
    async fn related_and_prevnext_render_when_seeded() {
        let (app, pool) = test_app().await;

        // Seed three posts with different timestamps
        for (slug, title, ts) in [
            ("prev-post", "Previous Post", 1_699_000_000_i64),
            ("main-post", "Main Post", 1_700_000_000_i64),
            ("next-post", "Next Post", 1_701_000_000_i64),
        ] {
            sqlx::query(
                r#"
                INSERT INTO posts (slug, title, status, author_id, published_at,
                                   updated_at, created_at, body_md, body_html,
                                   body_html_version, meta_json, assets_json)
                VALUES (?, ?, 'published', 1, ?, ?, ?,
                        '# x', '<p>body</p>', ?, '{}', '[]')
                "#,
            )
            .bind(slug)
            .bind(title)
            .bind(ts)
            .bind(ts)
            .bind(ts)
            .bind(content::RENDER_VERSION as i64)
            .execute(&pool)
            .await
            .unwrap();
        }

        let res = app
            .oneshot(
                Request::builder()
                    .uri("/posts/main-post")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let body = std::str::from_utf8(&bytes).unwrap();
        // prevnext section should have links to neighbors
        assert!(body.contains("class=\"prevnext\""), "prevnext nav missing");
        assert!(body.contains("/posts/prev-post"), "prev link missing");
        assert!(body.contains("/posts/next-post"), "next link missing");
        assert!(body.contains("Previous Post"), "prev title missing");
        assert!(body.contains("Next Post"), "next title missing");
        // related grid should contain the two other posts
        assert!(body.contains("class=\"related-grid\""), "related-grid missing");
    }
}
