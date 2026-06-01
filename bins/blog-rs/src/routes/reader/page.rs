//! GET /:slug — render a single published static page.
//!
//! This route is registered LAST in the reader router, so it acts as a
//! catch-all for slug-shaped paths. Reserved slugs that belong to real
//! application routes are rejected with 404 so this handler never shadows them.

use askama::Template;
use askama_axum::IntoResponse;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use db::pages::Page;

use crate::error::AppError;
use crate::routes::reader::error::ErrorTemplate;
use crate::state::AppState;
use crate::view::{AssetTag, SiteCtx};

/// Slugs that must not be captured by the static page route. These are all
/// handled by more-specific routes registered earlier in the router.
const RESERVED_SLUGS: &[&str] = &[
    "admin",
    "assets",
    "posts",
    "tags",
    "series",
    "search",
    "feed.xml",
    "sitemap.xml",
    "robots.txt",
    "members",
    "health",
];

#[derive(Debug, Clone)]
pub struct PageView {
    pub slug: String,
    pub title: String,
    pub body_html: String,
    pub toc: Vec<content::TocEntry>,
}

#[derive(Template)]
#[template(path = "reader/page.html")]
pub struct PageTemplate {
    pub site: SiteCtx,
    pub asset_tags: Vec<AssetTag>,
    pub nav: &'static str,
    pub page: PageView,
}

fn not_found(state: &AppState) -> axum::response::Response {
    let tpl = ErrorTemplate {
        site: SiteCtx {
            title: state.site.site_title.clone(),
            base_url: state.site.base_url.clone(),
            description: String::new(),
        },
        asset_tags: Vec::new(),
        nav: "",
    };
    (StatusCode::NOT_FOUND, tpl).into_response()
}

pub async fn handler(
    Path(slug): Path<String>,
    State(state): State<AppState>,
) -> Result<axum::response::Response, AppError> {
    // Guard: never let this handler shadow reserved application routes.
    if RESERVED_SLUGS.contains(&slug.as_str()) {
        return Ok(not_found(&state));
    }

    let mut page: Page = match db::pages::find_by_slug(&state.pool, &slug).await {
        Ok(p) if p.status == "published" => p,
        Ok(_) => {
            return Ok(not_found(&state));
        }
        Err(db::DbError::NotFound) => {
            return Ok(not_found(&state));
        }
        Err(e) => return Err(AppError::from(e)),
    };

    // Lazy regen: if the cached HTML was rendered by an older RENDER_VERSION,
    // re-render from body_md and persist before serving.
    if page.body_html_version < content::RENDER_VERSION as i64 {
        let out = content::render(&page.body_md)
            .map_err(|e| AppError::Internal(format!("re-render failed: {e}")))?;
        let toc_json = serde_json::to_string(&out.toc).unwrap_or_else(|_| "[]".into());
        db::pages::update_rendered_cache(
            &state.pool,
            page.id,
            &out.html,
            &toc_json,
            content::RENDER_VERSION as i64,
        )
        .await?;
        page.body_html = out.html;
        page.toc_json = toc_json;
    }

    let toc: Vec<content::TocEntry> =
        serde_json::from_str::<Vec<content::TocEntry>>(&page.toc_json).unwrap_or_default();

    let view = PageView {
        slug: page.slug.clone(),
        title: page.title.clone(),
        body_html: page.body_html.clone(),
        toc,
    };

    Ok(PageTemplate {
        site: SiteCtx::placeholder(),
        asset_tags: vec![],
        nav: "",
        page: view,
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
        let state = AppState::new(pool.clone(), Config::default(), vec![0u8; 32]);
        let app = crate::routes::router(state);
        (app, pool)
    }

    async fn seed_page(pool: &SqlitePool, slug: &str, status: &str) -> i64 {
        db::pages::create(
            pool,
            db::pages::NewPage {
                slug,
                title: "About This Blog",
                body_md: "# About\n\nThis is the about page.",
                body_html: "<h1>About</h1><p>This is the about page.</p>",
                toc_json: "[]",
                meta_json: None,
                status,
            },
        )
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn published_page_returns_200_with_body() {
        let (app, pool) = test_app().await;
        seed_page(&pool, "about", "published").await;

        let res = app
            .oneshot(
                Request::builder()
                    .uri("/about")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let body = std::str::from_utf8(&bytes).unwrap();
        assert!(body.contains("About This Blog"), "title missing: {body}");
        assert!(
            body.contains("This is the about page."),
            "body content missing"
        );
    }

    #[tokio::test]
    async fn draft_page_returns_404() {
        let (app, pool) = test_app().await;
        seed_page(&pool, "draft-page", "draft").await;

        let res = app
            .oneshot(
                Request::builder()
                    .uri("/draft-page")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn unknown_slug_returns_404() {
        let (app, _pool) = test_app().await;
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/no-such-page-xyz")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        // The fallback or this handler both return 404; either is correct.
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn search_route_not_shadowed_by_slug() {
        let (app, pool) = test_app().await;
        // Even if someone created a page slug "search", the /search route wins.
        seed_page(&pool, "search", "published").await;

        let res = app
            .oneshot(
                Request::builder()
                    .uri("/search")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        // /search should resolve to the search handler (200), not this page.
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let body = std::str::from_utf8(&bytes).unwrap();
        // The search page contains a search form; it should NOT be the page body.
        assert!(
            !body.contains("About This Blog"),
            "search slug should not resolve to page handler"
        );
    }

    #[tokio::test]
    async fn tags_route_not_shadowed_by_slug() {
        let (app, pool) = test_app().await;
        seed_page(&pool, "tags", "published").await;

        let res = app
            .oneshot(
                Request::builder()
                    .uri("/tags")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let body = std::str::from_utf8(&bytes).unwrap();
        assert!(
            !body.contains("About This Blog"),
            "tags slug should not resolve to page handler"
        );
    }

    #[tokio::test]
    async fn stale_version_triggers_lazy_regen() {
        let (app, pool) = test_app().await;
        // Insert a page with version=0 and stale body_html
        sqlx::query(
            "INSERT INTO pages (slug, title, body_md, body_html, body_html_version,
                                toc_json, status, created_at, updated_at)
             VALUES ('regen-me', 'Regen Page', '# Fresh Heading', '<p>STALE</p>',
                     0, '[]', 'published', 0, 0)",
        )
        .execute(&pool)
        .await
        .unwrap();

        let res = app
            .oneshot(
                Request::builder()
                    .uri("/regen-me")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let body = std::str::from_utf8(&bytes).unwrap();
        assert!(!body.contains("STALE"), "stale body_html should be replaced");
        assert!(body.contains("Fresh Heading"), "re-rendered content missing");
    }
}
