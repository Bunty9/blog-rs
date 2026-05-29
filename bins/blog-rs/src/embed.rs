//! Static assets embedded at compile time. Files under `blog-rs/assets/` ship
//! inside the binary; the handler serves them with sane MIME types.

use axum::body::Body;
use axum::extract::Path;
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::Response;
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "../../assets/"]
pub struct Assets;

pub async fn handler(Path(path): Path<String>) -> Response {
    match Assets::get(path.as_str()) {
        Some(file) => {
            let mime = mime_for(&path);
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, HeaderValue::from_static(mime))
                .header(header::CACHE_CONTROL, "public, max-age=86400")
                .body(Body::from(file.data.into_owned()))
                .unwrap()
        }
        None => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::empty())
            .unwrap(),
    }
}

fn mime_for(path: &str) -> &'static str {
    if path.ends_with(".css") {
        "text/css; charset=utf-8"
    } else if path.ends_with(".js") {
        "application/javascript; charset=utf-8"
    } else if path.ends_with(".svg") {
        "image/svg+xml"
    } else if path.ends_with(".png") {
        "image/png"
    } else if path.ends_with(".woff2") {
        "font/woff2"
    } else {
        "application/octet-stream"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mime_known_kinds() {
        assert!(mime_for("admin.css").starts_with("text/css"));
        assert!(mime_for("admin.js").starts_with("application/javascript"));
        assert!(mime_for("font.woff2").starts_with("font/woff2"));
        assert_eq!(mime_for("file.unknown"), "application/octet-stream");
    }
}
