//! GET  /admin/posts/:id/revisions                    — htmx sidebar partial.
//! POST /admin/posts/:id/revisions/:rev_id/restore     — restore a revision.
//!
//! Restoring first snapshots the post's *current* content (so the state
//! being replaced isn't lost), then writes the revision's content back
//! through the same `content::render` + `PostUpdate` path a normal save
//! uses (see `posts_save::render_body`), then tells htmx to reload the
//! editor via `HX-Redirect`.
//!
//! Auth + CSRF are enforced by the surrounding middleware.

use askama::Template;
use askama_axum::IntoResponse;
use axum::extract::{Path, State};
use axum::http::{HeaderValue, StatusCode};
use axum::response::Response;
use db::posts::{self, PostUpdate};
use db::revisions;

use super::posts_save::render_body;
use crate::error::AppError;
use crate::state::AppState;

/// View row for the sidebar list: the DB summary plus a display-formatted
/// timestamp (the DB layer stores unix seconds; formatting is a UI concern).
struct RevisionRow {
    id: i64,
    title: String,
    created_at_display: String,
    body_size: i64,
}

impl From<revisions::RevisionSummary> for RevisionRow {
    fn from(r: revisions::RevisionSummary) -> Self {
        Self {
            id: r.id,
            title: r.title,
            created_at_display: format_ts(r.created_at),
            body_size: r.body_size,
        }
    }
}

/// `2024-01-02 15:04 UTC` — good enough for a sidebar list; avoids pulling in
/// the `time` crate's `formatting` feature for one string.
fn format_ts(ts: i64) -> String {
    match time::OffsetDateTime::from_unix_timestamp(ts) {
        Ok(dt) => format!(
            "{:04}-{:02}-{:02} {:02}:{:02} UTC",
            dt.year(),
            dt.month() as u8,
            dt.day(),
            dt.hour(),
            dt.minute()
        ),
        Err(_) => ts.to_string(),
    }
}

#[derive(Template)]
#[template(path = "admin/partials/revisions_list.html")]
struct RevisionsListTpl {
    post_id: i64,
    revisions: Vec<RevisionRow>,
}

pub async fn list(
    State(state): State<AppState>,
    Path(post_id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    let revisions = revisions::list_for_post(&state.pool, post_id, 50)
        .await?
        .into_iter()
        .map(RevisionRow::from)
        .collect();
    Ok(RevisionsListTpl { post_id, revisions })
}

pub async fn restore(
    State(state): State<AppState>,
    Path((post_id, rev_id)): Path<(i64, i64)>,
) -> Result<Response, AppError> {
    let rev = revisions::get(&state.pool, rev_id)
        .await
        .map_err(|e| match e {
            db::DbError::NotFound => AppError::NotFound,
            other => AppError::from(other),
        })?;
    if rev.post_id != post_id {
        return Err(AppError::NotFound);
    }

    // Snapshot what's about to be overwritten so restoring is itself
    // reversible.
    let current = posts::find_by_id(&state.pool, post_id).await?;
    revisions::snapshot(
        &state.pool,
        post_id,
        &current.title,
        current.subtitle.as_deref(),
        &current.body_md,
        current.meta_json.as_deref(),
    )
    .await?;

    let mut update = PostUpdate {
        title: Some(rev.title),
        subtitle: Some(rev.subtitle.unwrap_or_default()),
        meta_json: Some(rev.meta_json.unwrap_or_else(|| "{}".into())),
        ..Default::default()
    };
    render_body(&rev.body_md, &mut update)?;
    posts::update_fields(&state.pool, post_id, &update).await?;

    let mut res = StatusCode::OK.into_response();
    res.headers_mut().insert(
        "HX-Redirect",
        HeaderValue::from_str(&format!("/admin/posts/{post_id}"))
            .unwrap_or_else(|_| HeaderValue::from_static("/admin/posts")),
    );
    Ok(res)
}

#[cfg(test)]
mod tests {
    use crate::config::Config;
    use axum::body::{to_bytes, Body};
    use axum::http::{header, Request, StatusCode};
    use db::test_support::fresh_pool;
    use tower::ServiceExt;

    async fn test_app() -> (axum::Router, crate::state::AppState) {
        let pool = fresh_pool().await;
        let state = crate::state::AppState::new(pool, Config::default(), vec![0u8; 32]);
        let app = crate::routes::router(state.clone());
        (app, state)
    }

    async fn seed_admin_session(state: &crate::state::AppState) -> (String, String) {
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

    fn cookie(sid: &str, csrf: &str) -> String {
        format!(
            "{}={}; {}={}",
            auth::session::SESSION_COOKIE,
            sid,
            auth::session::CSRF_COOKIE,
            csrf
        )
    }

    async fn seed_post(state: &crate::state::AppState, slug: &str, title: &str) -> i64 {
        sqlx::query(
            "INSERT INTO users (id, email, password_hash, role, created_at) \
             VALUES (1, 'a@b', 'x', 'admin', 0) ON CONFLICT(id) DO NOTHING",
        )
        .execute(&state.pool)
        .await
        .unwrap();
        sqlx::query_scalar::<_, i64>(
            r#"
            INSERT INTO posts (slug, title, status, author_id,
                               updated_at, created_at, body_md, body_html,
                               meta_json, assets_json)
            VALUES (?, ?, 'draft', 1, 0, 0, '# original', '<h1>original</h1>', '{}', '[]')
            RETURNING id
            "#,
        )
        .bind(slug)
        .bind(title)
        .fetch_one(&state.pool)
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn list_requires_auth() {
        let (app, state) = test_app().await;
        let post_id = seed_post(&state, "p", "P").await;
        let res = app
            .oneshot(
                Request::builder()
                    .uri(format!("/admin/posts/{post_id}/revisions"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn list_renders_revisions_newest_first() {
        let (app, state) = test_app().await;
        let (sid, csrf) = seed_admin_session(&state).await;
        let post_id = seed_post(&state, "p", "P").await;
        db::revisions::snapshot(&state.pool, post_id, "Old title", None, "# old body", None)
            .await
            .unwrap();

        let res = app
            .oneshot(
                Request::builder()
                    .uri(format!("/admin/posts/{post_id}/revisions"))
                    .header(header::COOKIE, cookie(&sid, &csrf))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let body = std::str::from_utf8(&bytes).unwrap();
        assert!(body.contains("Old title"), "revision missing: {body}");
    }

    #[tokio::test]
    async fn restore_writes_body_and_snapshots_pre_restore_content() {
        let (app, state) = test_app().await;
        let (sid, csrf) = seed_admin_session(&state).await;
        let post_id = seed_post(&state, "p", "P").await;
        let rev_id = db::revisions::snapshot(
            &state.pool,
            post_id,
            "Restored title",
            Some("restored sub"),
            "# restored body",
            Some(r#"{"series":"s"}"#),
        )
        .await
        .unwrap();

        let res = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/admin/posts/{post_id}/revisions/{rev_id}/restore"))
                    .header(header::COOKIE, cookie(&sid, &csrf))
                    .header("x-csrf-token", &csrf)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(
            res.headers().get("HX-Redirect").unwrap(),
            &format!("/admin/posts/{post_id}")
        );

        let restored = db::posts::find_by_id(&state.pool, post_id).await.unwrap();
        assert_eq!(restored.title, "Restored title");
        assert_eq!(restored.subtitle.as_deref(), Some("restored sub"));
        assert_eq!(restored.body_md, "# restored body");
        assert!(restored.body_html.contains("restored body"));
        assert_eq!(restored.meta_json.as_deref(), Some(r#"{"series":"s"}"#));

        // The pre-restore content ("# original") was itself snapshotted.
        let revs = db::revisions::list_for_post(&state.pool, post_id, 50)
            .await
            .unwrap();
        assert_eq!(revs.len(), 2, "restore should snapshot the prior content");
        let pre_restore = db::revisions::get(&state.pool, revs[0].id).await.unwrap();
        assert_eq!(pre_restore.body_md, "# original");
    }

    #[tokio::test]
    async fn restore_rejects_revision_from_another_post() {
        let (app, state) = test_app().await;
        let (sid, csrf) = seed_admin_session(&state).await;
        let post_id = seed_post(&state, "p", "P").await;
        let other_post_id = seed_post(&state, "other", "Other").await;
        let rev_id = db::revisions::snapshot(
            &state.pool,
            other_post_id,
            "Other's revision",
            None,
            "# other body",
            None,
        )
        .await
        .unwrap();

        let res = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/admin/posts/{post_id}/revisions/{rev_id}/restore"))
                    .header(header::COOKIE, cookie(&sid, &csrf))
                    .header("x-csrf-token", &csrf)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);

        // The target post's content must be untouched.
        let post = db::posts::find_by_id(&state.pool, post_id).await.unwrap();
        assert_eq!(post.body_md, "# original");
    }

    #[tokio::test]
    async fn restore_without_csrf_is_rejected() {
        let (app, state) = test_app().await;
        let (sid, csrf) = seed_admin_session(&state).await;
        let post_id = seed_post(&state, "p", "P").await;
        let rev_id = db::revisions::snapshot(&state.pool, post_id, "T", None, "# t", None)
            .await
            .unwrap();

        let res = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/admin/posts/{post_id}/revisions/{rev_id}/restore"))
                    .header(header::COOKIE, cookie(&sid, &csrf))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::FORBIDDEN);
    }
}
