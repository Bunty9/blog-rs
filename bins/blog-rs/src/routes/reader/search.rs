use crate::state::AppState;
use crate::view::{iso_date, AssetTag, Pagination, SiteCtx, PAGE_SIZE};
use askama::Template;
use askama_axum::IntoResponse;
use axum::extract::{Query, State};
use serde::Deserialize;

/// Limit for instant/partial results (no pagination needed for the htmx fragment).
const INSTANT_LIMIT: i64 = 20;

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    #[serde(default)]
    pub q: String,
    pub page: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct SearchHitView {
    pub slug: String,
    pub title: String,
    pub snippet: String,
    pub published_date: Option<String>,
}

#[derive(Template)]
#[template(path = "reader/search.html")]
pub struct SearchTemplate {
    pub site: SiteCtx,
    pub asset_tags: Vec<AssetTag>,
    pub nav: &'static str,
    pub query: String,
    pub hits: Vec<SearchHitView>,
    pub pagination: Pagination,
}

/// Fragment-only template returned by `/search/instant`.
/// Does NOT extend base.html — it is a bare HTML fragment for htmx to swap in.
#[derive(Template)]
#[template(path = "partials/search_results.html")]
pub struct SearchResultsTemplate {
    pub query: String,
    pub hits: Vec<SearchHitView>,
}

pub async fn handler(
    Query(q): Query<SearchQuery>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, crate::error::AppError> {
    let query = q.q.trim().to_string();
    let (hits, total) = if query.is_empty() {
        (Vec::new(), 0i64)
    } else {
        // Pull one page + tiny lookahead for total. For Phase 1c we don't
        // count total matches separately; pagination uses returned-row-count
        // heuristic. To get a precise total we'd need a COUNT(*) over the FTS
        // join, which is fine but spec-quiet — leaving simple for now.
        let rows = db::posts::search_fts(&state.pool, &query, PAGE_SIZE * 10, 0)
            .await
            .map_err(db::DbError::from)?;
        let total = rows.len() as i64;
        let page = crate::view::clamp_page(q.page, total);
        let offset = ((page - 1) * PAGE_SIZE) as usize;
        let slice: Vec<_> = rows
            .into_iter()
            .skip(offset)
            .take(PAGE_SIZE as usize)
            .collect();
        (slice, total)
    };

    let hit_views: Vec<SearchHitView> = hits
        .iter()
        .map(|h| SearchHitView {
            slug: h.slug.clone(),
            title: h.title.clone(),
            snippet: h.snippet.clone(),
            published_date: h.published_at.map(iso_date),
        })
        .collect();

    let suffix = if query.is_empty() {
        String::new()
    } else {
        format!("&q={}", urlencoding::encode(&query))
    };
    let page = crate::view::clamp_page(q.page, total);
    let pagination = Pagination::new(page, total, "/search").with_query_suffix(suffix);

    Ok(SearchTemplate {
        site: SiteCtx::placeholder(),
        asset_tags: Vec::new(),
        nav: "",
        query,
        hits: hit_views,
        pagination,
    })
}

/// `GET /search/instant?q=<query>` — returns the bare `search_results.html` fragment
/// for htmx to swap into `#search-results`. No base layout, no `<html>` wrapper.
pub async fn instant_handler(
    Query(q): Query<SearchQuery>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, crate::error::AppError> {
    let query = q.q.trim().to_string();
    let hits = if query.is_empty() {
        Vec::new()
    } else {
        let rows = db::posts::search_fts(&state.pool, &query, INSTANT_LIMIT, 0)
            .await
            .map_err(db::DbError::from)?;
        rows.iter()
            .map(|h| SearchHitView {
                slug: h.slug.clone(),
                title: h.title.clone(),
                snippet: h.snippet.clone(),
                published_date: h.published_at.map(iso_date),
            })
            .collect()
    };
    Ok(SearchResultsTemplate { query, hits })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use sqlx::SqlitePool;
    use tower::ServiceExt;

    async fn test_app() -> (axum::Router, SqlitePool) {
        let pool = db::test_support::fresh_pool().await;
        sqlx::query(
            "INSERT INTO users (id, email, password_hash, role, created_at) \
             VALUES (1, 'a@b', 'x', 'admin', 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        let state = AppState::new(pool.clone(), Config::default(), vec![0u8; 32]);
        (crate::routes::router(state), pool)
    }

    async fn insert(pool: &SqlitePool, slug: &str, title: &str, body: &str) {
        sqlx::query(
            r#"INSERT INTO posts (slug, title, status, author_id, published_at, updated_at, created_at, body_md, body_html, meta_json, assets_json, excerpt)
               VALUES (?, ?, 'published', 1, 1700000000, 1700000000, 1700000000, ?, '<h1>x</h1>', '{}', '[]', '')"#,
        )
        .bind(slug)
        .bind(title)
        .bind(body)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn body_of(app: axum::Router, uri: &str) -> (StatusCode, String) {
        let res = app
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = res.status();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        (status, String::from_utf8(bytes.to_vec()).unwrap())
    }

    #[tokio::test]
    async fn empty_query_renders_search_box() {
        let (app, _pool) = test_app().await;
        let (status, body) = body_of(app, "/search").await;
        assert_eq!(status, StatusCode::OK);
        // The search input with htmx attributes must be present.
        assert!(body.contains(r#"id="q""#));
        assert!(body.contains("hx-get"));
    }

    #[tokio::test]
    async fn query_with_no_matches_renders_empty_message() {
        let (app, pool) = test_app().await;
        insert(&pool, "p", "Title", "body text").await;
        let (status, body) = body_of(app, "/search?q=zzzzzzz").await;
        assert_eq!(status, StatusCode::OK);
        assert!(body.contains("No matches for"));
    }

    #[tokio::test]
    async fn query_with_match_returns_snippet_with_mark() {
        let (app, pool) = test_app().await;
        insert(
            &pool,
            "boot-up",
            "Booting Cortex-M4",
            "Embedded Rust runs on Cortex hardware.",
        )
        .await;
        let (status, body) = body_of(app, "/search?q=Cortex").await;
        assert_eq!(status, StatusCode::OK);
        assert!(body.contains("/posts/boot-up"));
        assert!(body.contains("<mark>"));
    }

    #[tokio::test]
    async fn fts_special_chars_do_not_500() {
        let (app, pool) = test_app().await;
        insert(&pool, "p", "T", "body").await;
        for q in ["*", "AND", "(", "rust*"] {
            let (status, _body) = body_of(
                app.clone(),
                &format!("/search?q={}", urlencoding::encode(q)),
            )
            .await;
            assert_eq!(status, StatusCode::OK, "{q} should not 500");
        }
    }

    // ── instant / htmx fragment tests ──────────────────────────────────────

    #[tokio::test]
    async fn instant_empty_query_returns_200_empty_fragment() {
        let (app, _pool) = test_app().await;
        let (status, body) = body_of(app, "/search/instant").await;
        assert_eq!(status, StatusCode::OK);
        // Must NOT contain <html> or page layout — it is a bare fragment.
        assert!(!body.contains("<html"));
        assert!(!body.contains("<header"));
        // No results markup for empty query.
        assert!(!body.contains("search-result"));
    }

    #[tokio::test]
    async fn instant_query_returns_fragment_with_hit() {
        let (app, pool) = test_app().await;
        insert(
            &pool,
            "instant-post",
            "Instant Rust Guide",
            "A guide to instant feedback loops in Rust.",
        )
        .await;
        let (status, body) = body_of(app, "/search/instant?q=instant").await;
        assert_eq!(status, StatusCode::OK);
        // Fragment: no full-page layout.
        assert!(!body.contains("<html"));
        assert!(!body.contains("<header"));
        // Hit present with slug link.
        assert!(body.contains("/posts/instant-post"), "expected slug link in body: {body}");
        // Snippet should contain <mark> from FTS5.
        assert!(body.contains("<mark>"), "expected <mark> in snippet: {body}");
    }

    #[tokio::test]
    async fn instant_no_match_returns_empty_state() {
        let (app, pool) = test_app().await;
        insert(&pool, "p", "Title", "body text").await;
        let (status, body) = body_of(app, "/search/instant?q=zzznomatch").await;
        assert_eq!(status, StatusCode::OK);
        assert!(!body.contains("<html"));
        assert!(body.contains("No matches for"));
    }
}
