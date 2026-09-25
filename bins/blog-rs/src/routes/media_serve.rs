//! GET /media/:filename — public media serving. No auth: these are the
//! images referenced by published post/page bodies.
//!
//! The filename is validated strictly before it ever touches the filesystem
//! or the DB (`crate::media::valid_filename`: 16 lowercase-hex chars, a dot,
//! one of the four allowed extensions). That alone rules out `..` and `/` in
//! the segment, so a traversal attempt just falls through to the same 404 as
//! an unknown file.

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::response::Response;

use crate::state::AppState;

pub async fn handler(State(state): State<AppState>, Path(filename): Path<String>) -> Response {
    let Some(content_type) = crate::media::valid_filename(&filename) else {
        return not_found();
    };
    if db::media::find_by_filename(&state.pool, &filename)
        .await
        .is_err()
    {
        return not_found();
    }
    let path = state.site.media_dir.join(&filename);
    let Ok(bytes) = tokio::fs::read(&path).await else {
        return not_found();
    };
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CACHE_CONTROL, "public, max-age=31536000, immutable")
        .header("X-Content-Type-Options", "nosniff")
        .body(Body::from(bytes))
        .unwrap()
}

fn not_found() -> Response {
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(Body::empty())
        .unwrap()
}

#[cfg(test)]
mod tests {
    use crate::config::Config;
    use crate::state::{AppState, SiteConfig};
    use axum::body::{to_bytes, Body};
    use axum::http::{Request, StatusCode};
    use db::test_support::fresh_pool;
    use tower::ServiceExt;

    async fn seeded_app() -> (axum::Router, tempfile::TempDir, String) {
        let pool = fresh_pool().await;
        let tmp = tempfile::tempdir().unwrap();
        let filename = "0123456789abcdef.png";
        std::fs::write(tmp.path().join(filename), b"fake-png-bytes").unwrap();
        db::media::insert(
            &pool,
            db::media::NewMedia {
                filename,
                original_name: "cat.png",
                content_type: "image/png",
                size_bytes: 14,
                sha256: "0123456789abcdef0000000000000000000000000000000000000000000000",
                width: None,
                height: None,
            },
        )
        .await
        .unwrap();

        let state = AppState::new(pool, Config::default(), vec![0u8; 32]).with_site(SiteConfig {
            media_dir: tmp.path().to_path_buf(),
            ..SiteConfig::default()
        });
        let app = crate::routes::router(state);
        (app, tmp, filename.to_string())
    }

    #[tokio::test]
    async fn serves_known_file_with_headers() {
        let (app, _tmp, filename) = seeded_app().await;
        let res = app
            .oneshot(
                Request::builder()
                    .uri(format!("/media/{filename}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(res.headers().get("content-type").unwrap(), "image/png");
        assert_eq!(
            res.headers().get("x-content-type-options").unwrap(),
            "nosniff"
        );
        assert_eq!(
            res.headers().get("cache-control").unwrap(),
            "public, max-age=31536000, immutable"
        );
        let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
        assert_eq!(&bytes[..], b"fake-png-bytes");
    }

    #[tokio::test]
    async fn unknown_file_is_404() {
        let (app, _tmp, _filename) = seeded_app().await;
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/media/ffffffffffffffff.png")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn traversal_attempt_is_404() {
        let (app, _tmp, _filename) = seeded_app().await;
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/media/..%2f..%2fetc%2fpasswd")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
    }
}
