use crate::state::AppState;
use crate::view::{AssetTag, SiteCtx};
use askama::Template;
use askama_axum::IntoResponse;
use axum::extract::State;
use axum::http::StatusCode;

#[derive(Template)]
#[template(path = "reader/error.html")]
pub struct ErrorTemplate {
    pub site: SiteCtx,
    pub asset_tags: Vec<AssetTag>,
    pub nav: &'static str,
}

pub async fn fallback(State(state): State<AppState>) -> impl IntoResponse {
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

#[cfg(test)]
mod tests {
    use crate::config::Config;
    use crate::state::AppState;
    use axum::body::{to_bytes, Body};
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    async fn test_app() -> axum::Router {
        let pool = db::test_support::fresh_pool().await;
        sqlx::query(
            "INSERT INTO users (id, email, password_hash, role, created_at) \
             VALUES (1, 'a@b', 'x', 'admin', 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        let state = AppState::new(pool.clone(), Config::default(), vec![0u8; 32]);
        crate::routes::router(state)
    }

    #[tokio::test]
    async fn unknown_route_returns_404_with_error_template() {
        let app = test_app().await;
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/this-route-does-not-exist-at-all")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
        let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let body = std::str::from_utf8(&bytes).unwrap();
        assert!(
            body.contains("404"),
            "404 text missing from error page: {body}"
        );
        assert!(
            body.contains("action=\"/search\"") || body.contains("action=/search"),
            "search box missing from error page: {body}"
        );
    }
}
