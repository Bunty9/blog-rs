use crate::view::SiteCtx;
use axum::http::{header, HeaderValue};
use axum::response::{IntoResponse, Response};

pub async fn handler() -> Response {
    let site = SiteCtx::placeholder();
    let body = format!(
        "User-agent: *\nAllow: /\nDisallow: /admin\nDisallow: /search\nSitemap: {base}/sitemap.xml\n",
        base = site.base_url,
    );
    let mut res = body.into_response();
    res.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    res
}

#[cfg(test)]
mod tests {
    use crate::config::Config;
    use crate::state::AppState;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    async fn test_app() -> axum::Router {
        let pool = db::test_support::fresh_pool().await;
        let state = AppState::new(pool, Config::default(), vec![0u8; 32]);
        crate::routes::router(state)
    }

    #[tokio::test]
    async fn robots_serves_plain_text_with_sitemap_link() {
        let app = test_app().await;
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/robots.txt")
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
            .starts_with("text/plain"));
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let body = std::str::from_utf8(&bytes).unwrap();
        assert!(body.contains("User-agent: *"));
        assert!(body.contains("Sitemap:"));
        assert!(body.contains("/sitemap.xml"));
        assert!(body.contains("Disallow: /admin"));
    }

    #[tokio::test]
    async fn robots_disallows_search() {
        let app = test_app().await;
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/robots.txt")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let body = std::str::from_utf8(&bytes).unwrap();
        assert!(body.contains("Disallow: /search"));
    }
}
