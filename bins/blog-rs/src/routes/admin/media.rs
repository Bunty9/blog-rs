//! GET  /admin/media               — media library grid.
//! POST /admin/media                — upload a file (multipart), dedupes by
//!                                    content hash. Body capped at 10 MiB by
//!                                    a `DefaultBodyLimit` layer on the route.
//! POST /admin/media/:id/alt        — save alt text.
//! POST /admin/media/:id/delete     — remove the DB row and the file on disk.
//! GET  /admin/media/picker         — htmx partial listing media for the
//!                                    post editor's insert-image dialog.
//!
//! Auth + CSRF for the mutating routes are enforced by the surrounding
//! middleware (`routes::admin::router`); the picker is a GET so only auth
//! applies. Uploaded content type is decided purely from magic bytes
//! (`crate::media::sniff`), never from the client's filename or declared
//! Content-Type — that's what keeps SVG and other script-capable types out.

use askama::Template;
use askama_axum::IntoResponse;
use axum::extract::{Multipart, Path, State};
use axum::Extension;
use axum::Form;
use serde::Deserialize;

use crate::error::AppError;
use crate::middleware::auth_required::SessionCtx;
use crate::state::AppState;
use db::media::{self, NewMedia};

/// View model for a media row, adding the two things templates need that
/// aren't in the DB row: a human file size (via askama's `filesizeformat`
/// needs a plain number, which `size_bytes` already is) and the ready-to-copy
/// shortcode snippet.
struct MediaRow {
    id: i64,
    filename: String,
    original_name: String,
    alt: String,
    size_bytes: i64,
    width: Option<i64>,
    height: Option<i64>,
    shortcode: String,
}

impl From<db::Media> for MediaRow {
    fn from(m: db::Media) -> Self {
        let shortcode = format!(
            r#"{{{{< image src="/media/{}" alt="{}" >}}}}"#,
            m.filename, m.alt
        );
        Self {
            id: m.id,
            filename: m.filename,
            original_name: m.original_name,
            alt: m.alt,
            size_bytes: m.size_bytes,
            width: m.width,
            height: m.height,
            shortcode,
        }
    }
}

const PAGE_SIZE: i64 = 60;

async fn rows(state: &AppState) -> Result<Vec<MediaRow>, AppError> {
    Ok(media::list(&state.pool, PAGE_SIZE, 0)
        .await?
        .into_iter()
        .map(MediaRow::from)
        .collect())
}

#[derive(Template)]
#[template(path = "admin/media.html")]
struct MediaTpl {
    csrf: String,
    nav: &'static str,
    page_title: &'static str,
    flash: Option<String>,
    flash_kind: String,
    rows: Vec<MediaRow>,
}

pub async fn handler(
    State(state): State<AppState>,
    Extension(session): Extension<SessionCtx>,
) -> Result<impl IntoResponse, AppError> {
    Ok(MediaTpl {
        csrf: session.csrf_token,
        nav: "media",
        page_title: "Media",
        flash: None,
        flash_kind: String::new(),
        rows: rows(&state).await?,
    })
}

#[derive(Template)]
#[template(path = "admin/partials/media_items.html")]
struct MediaItemsTpl {
    rows: Vec<MediaRow>,
}

pub async fn upload(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, AppError> {
    // Extract everything we need from each field before looping again — a
    // `Field` borrows `multipart` mutably, so it can't be held across the
    // next `next_field().await` call.
    let mut found = None;
    while let Some(f) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(format!("bad multipart body: {e}")))?
    {
        if f.name() == Some("file") {
            let original_name = f.file_name().unwrap_or("upload").to_string();
            let data = f
                .bytes()
                .await
                .map_err(|e| AppError::BadRequest(format!("failed to read upload: {e}")))?;
            found = Some((original_name, data));
            break;
        }
    }
    let (original_name, data) =
        found.ok_or_else(|| AppError::BadRequest("missing `file` field".into()))?;

    let Some((content_type, ext)) = crate::media::sniff(&data) else {
        return Err(AppError::UnsupportedMediaType(
            "unsupported file type — only PNG, JPEG, GIF, and WebP are allowed".into(),
        ));
    };
    let sha256 = crate::media::sha256_hex(&data);
    let filename = format!("{}.{ext}", &sha256[..16]);
    let (width, height) = crate::media::sniff_dims(content_type, &data);

    // File first, then row: a failed write must not leave a row pointing at
    // nothing. The name is content-addressed, so an existing file already
    // holds these bytes and is left alone — rewriting it in place would let a
    // concurrent public GET (cached `immutable` for a year) read a torn file.
    let path = state.site.media_dir.join(&filename);
    if tokio::fs::metadata(&path).await.is_err() {
        let tmp = state
            .site
            .media_dir
            .join(format!(".{}.tmp", uuid::Uuid::new_v4()));
        let write = async {
            tokio::fs::write(&tmp, &data).await?;
            tokio::fs::rename(&tmp, &path).await
        };
        if let Err(e) = write.await {
            let _ = tokio::fs::remove_file(&tmp).await;
            return Err(AppError::Internal(format!(
                "failed to write media file: {e}"
            )));
        }
    }

    media::insert(
        &state.pool,
        NewMedia {
            filename: &filename,
            original_name: &original_name,
            content_type,
            size_bytes: data.len() as i64,
            sha256: &sha256,
            width,
            height,
        },
    )
    .await?;

    Ok(MediaItemsTpl {
        rows: rows(&state).await?,
    })
}

#[derive(Debug, Deserialize)]
pub struct AltForm {
    #[serde(default)]
    alt: String,
}

#[derive(Template)]
#[template(path = "admin/partials/media_item.html")]
struct MediaItemTpl {
    row: MediaRow,
}

pub async fn set_alt(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Form(form): Form<AltForm>,
) -> Result<impl IntoResponse, AppError> {
    // The alt is pasted verbatim into `alt="…"` shortcode snippets, and the
    // shortcode args parser has no escapes: a `"` or `}}` would break the
    // render of every post that uses the image.
    let alt = form.alt.replace('"', "'").replace("}}", "");
    media::update_alt(&state.pool, id, alt.trim()).await?;
    let row = media::find_by_id(&state.pool, id).await?.into();
    Ok(MediaItemTpl { row })
}

pub async fn delete(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    let row = media::find_by_id(&state.pool, id).await?;
    media::delete(&state.pool, id).await?;
    let _ = tokio::fs::remove_file(state.site.media_dir.join(&row.filename)).await;
    Ok(())
}

#[derive(Template)]
#[template(path = "admin/partials/media_picker.html")]
struct MediaPickerTpl {
    rows: Vec<MediaRow>,
}

pub async fn picker(State(state): State<AppState>) -> Result<impl IntoResponse, AppError> {
    Ok(MediaPickerTpl {
        rows: rows(&state).await?,
    })
}

#[cfg(test)]
mod tests {
    use crate::config::Config;
    use crate::state::{AppState, SiteConfig};
    use axum::body::{to_bytes, Body};
    use axum::http::{header, Request, StatusCode};
    use db::test_support::fresh_pool;
    use tower::ServiceExt;

    async fn test_app() -> (axum::Router, AppState, tempfile::TempDir) {
        let pool = fresh_pool().await;
        let tmp = tempfile::tempdir().unwrap();
        let state = AppState::new(pool, Config::default(), vec![0u8; 32]).with_site(SiteConfig {
            media_dir: tmp.path().to_path_buf(),
            ..SiteConfig::default()
        });
        let app = crate::routes::router(state.clone());
        (app, state, tmp)
    }

    async fn seed_admin_session(state: &AppState) -> (String, String) {
        let hash = auth::password::hash("hunter2").unwrap();
        db::users::bootstrap_admin(&state.pool, "admin@example.com", &hash)
            .await
            .unwrap();
        let user_id = db::users::find_by_email(&state.pool, "admin@example.com")
            .await
            .unwrap()
            .id;
        let session_token = auth::session::mint_token();
        let csrf = auth::session::mint_token();
        let expires = time::OffsetDateTime::now_utc().unix_timestamp() + 3600;
        db::sessions::create(&state.pool, &session_token, user_id, &csrf, expires)
            .await
            .unwrap();
        (session_token, csrf)
    }

    fn cookie(sid: &str) -> String {
        format!("{}={}", auth::session::SESSION_COOKIE, sid)
    }

    // A minimal, valid 1x1 PNG.
    const TINY_PNG: &[u8] = &[
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f,
        0x15, 0xc4, 0x89, 0x00, 0x00, 0x00, 0x0a, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0x00,
        0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0d, 0x0a, 0x2d, 0xb4, 0x00, 0x00, 0x00, 0x00, 0x49,
        0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
    ];

    fn multipart_body(boundary: &str, filename: &str, content: &[u8]) -> Vec<u8> {
        let mut body = Vec::new();
        body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        body.extend_from_slice(
            format!("Content-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\n")
                .as_bytes(),
        );
        body.extend_from_slice(b"Content-Type: application/octet-stream\r\n\r\n");
        body.extend_from_slice(content);
        body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
        body
    }

    async fn upload_png(app: &axum::Router, sid: &str, csrf: &str) -> axum::http::Response<Body> {
        let boundary = "X-BOUNDARY-X";
        let body = multipart_body(boundary, "cat.png", TINY_PNG);
        app.clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/admin/media")
                    .header(header::COOKIE, cookie(sid))
                    .header("x-csrf-token", csrf)
                    .header(
                        header::CONTENT_TYPE,
                        format!("multipart/form-data; boundary={boundary}"),
                    )
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn media_unauth_returns_401() {
        let (app, _state, _tmp) = test_app().await;
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/admin/media")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn media_auth_renders_empty_state() {
        let (app, state, _tmp) = test_app().await;
        let (sid, _csrf) = seed_admin_session(&state).await;

        let res = app
            .oneshot(
                Request::builder()
                    .uri("/admin/media")
                    .header(header::COOKIE, cookie(&sid))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let body = std::str::from_utf8(&bytes).unwrap();
        assert!(body.contains("Media"), "title missing: {body}");
        assert!(
            body.contains("No media yet"),
            "empty state text missing: {body}"
        );
    }

    #[tokio::test]
    async fn upload_png_succeeds_and_lists_it() {
        let (app, state, tmp) = test_app().await;
        let (sid, csrf) = seed_admin_session(&state).await;

        let res = upload_png(&app, &sid, &csrf).await;
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let body = std::str::from_utf8(&bytes).unwrap();
        assert!(
            body.contains(".png"),
            "expected filename in response: {body}"
        );
        assert!(
            body.contains("image\" src=\"/media/") || body.contains("src=\"/media/"),
            "expected media item image tag: {body}"
        );

        let rows = db::media::list(&state.pool, 10, 0).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert!(tmp.path().join(&rows[0].filename).exists());
    }

    #[tokio::test]
    async fn upload_dedupes_same_bytes() {
        let (app, state, _tmp) = test_app().await;
        let (sid, csrf) = seed_admin_session(&state).await;

        upload_png(&app, &sid, &csrf).await;
        upload_png(&app, &sid, &csrf).await;

        let rows = db::media::list(&state.pool, 10, 0).await.unwrap();
        assert_eq!(rows.len(), 1);
    }

    #[tokio::test]
    async fn upload_text_rejected_415() {
        let (app, state, _tmp) = test_app().await;
        let (sid, csrf) = seed_admin_session(&state).await;

        let boundary = "X-BOUNDARY-X";
        let body = multipart_body(boundary, "notes.txt", b"just some text");
        let res = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/admin/media")
                    .header(header::COOKIE, cookie(&sid))
                    .header("x-csrf-token", &csrf)
                    .header(
                        header::CONTENT_TYPE,
                        format!("multipart/form-data; boundary={boundary}"),
                    )
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);

        let rows = db::media::list(&state.pool, 10, 0).await.unwrap();
        assert!(rows.is_empty());
    }

    #[tokio::test]
    async fn upload_without_csrf_rejected() {
        let (app, state, _tmp) = test_app().await;
        let (sid, _csrf) = seed_admin_session(&state).await;

        let boundary = "X-BOUNDARY-X";
        let body = multipart_body(boundary, "cat.png", TINY_PNG);
        let res = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/admin/media")
                    .header(header::COOKIE, cookie(&sid))
                    .header(
                        header::CONTENT_TYPE,
                        format!("multipart/form-data; boundary={boundary}"),
                    )
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn delete_removes_row_and_file() {
        let (app, state, tmp) = test_app().await;
        let (sid, csrf) = seed_admin_session(&state).await;
        upload_png(&app, &sid, &csrf).await;
        let row = db::media::list(&state.pool, 10, 0).await.unwrap()[0].clone();
        assert!(tmp.path().join(&row.filename).exists());

        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/admin/media/{}/delete", row.id))
                    .header(header::COOKIE, cookie(&sid))
                    .header("x-csrf-token", &csrf)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        let err = db::media::find_by_id(&state.pool, row.id)
            .await
            .unwrap_err();
        assert!(matches!(err, db::DbError::NotFound));
        assert!(!tmp.path().join(&row.filename).exists());
    }

    #[tokio::test]
    async fn set_alt_updates_text() {
        let (app, state, _tmp) = test_app().await;
        let (sid, csrf) = seed_admin_session(&state).await;
        upload_png(&app, &sid, &csrf).await;
        let row = db::media::list(&state.pool, 10, 0).await.unwrap()[0].clone();

        let res = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/admin/media/{}/alt", row.id))
                    .header(header::COOKIE, cookie(&sid))
                    .header("x-csrf-token", &csrf)
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .body(Body::from("alt=a+%22happy%22+cat%7D%7D"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        let updated = db::media::find_by_id(&state.pool, row.id).await.unwrap();
        // Quotes and `}}` would break the shortcode snippet built from alt.
        assert_eq!(updated.alt, "a 'happy' cat");
    }
}
