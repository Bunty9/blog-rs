use crate::routes::reader::home::PostCard;
use crate::state::AppState;
use crate::view::{AssetTag, SiteCtx};
use askama::Template;
use askama_axum::IntoResponse;
use axum::extract::{Path, State};
use axum::http::StatusCode;

#[derive(Debug, Clone)]
pub struct SeriesView {
    pub slug: String,
}

#[derive(Template)]
#[template(path = "reader/series.html")]
pub struct SeriesTemplate {
    pub site: SiteCtx,
    pub asset_tags: Vec<AssetTag>,
    pub series: SeriesView,
    pub cards: Vec<PostCard>,
}

pub async fn handler(
    Path(slug): Path<String>,
    State(state): State<AppState>,
) -> Result<axum::response::Response, crate::error::AppError> {
    let meta = match db::series::get_meta(&state.pool, &slug)
        .await
        .map_err(db::DbError::from)?
    {
        Some(m) => m,
        None => {
            return Ok((StatusCode::NOT_FOUND, "404 — series not found").into_response());
        }
    };
    let rows = db::posts::list_by_series(&state.pool, &slug)
        .await
        .map_err(db::DbError::from)?;
    let cards: Vec<PostCard> = rows.iter().map(PostCard::from).collect();

    Ok(SeriesTemplate {
        site: SiteCtx::placeholder(),
        asset_tags: Vec::new(),
        series: SeriesView { slug: meta.slug },
        cards,
    }
    .into_response())
}

#[cfg(test)]
mod tests {
    use crate::config::Config;
    use crate::state::AppState;
    use axum::body::{to_bytes, Body};
    use axum::http::{Request, StatusCode};
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

    async fn insert(pool: &SqlitePool, slug: &str, title: &str, meta: &str) {
        sqlx::query(
            r#"INSERT INTO posts (slug, title, status, author_id, published_at, updated_at, created_at, body_md, body_html, meta_json, assets_json)
               VALUES (?, ?, 'published', 1, 1700000000, 1700000000, 1700000000, '#x', '<h1>x</h1>', ?, '[]')"#,
        )
        .bind(slug)
        .bind(title)
        .bind(meta)
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn series_orders_by_series_order() {
        let (app, pool) = test_app().await;
        insert(
            &pool,
            "p2",
            "Two",
            r#"{"series":"rust-level-4","series_order":2}"#,
        )
        .await;
        insert(
            &pool,
            "p1",
            "One",
            r#"{"series":"rust-level-4","series_order":1}"#,
        )
        .await;

        let res = app
            .oneshot(
                Request::builder()
                    .uri("/series/rust-level-4")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let body = std::str::from_utf8(&bytes).unwrap();
        let idx_one = body.find("One").unwrap();
        let idx_two = body.find("Two").unwrap();
        assert!(
            idx_one < idx_two,
            "expected 'One' before 'Two' in series ordering"
        );
    }

    #[tokio::test]
    async fn unknown_series_404() {
        let (app, _pool) = test_app().await;
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/series/no-such")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
    }
}
