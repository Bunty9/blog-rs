//! Black-box-ish integration test for the outbox worker tick. Plants pending
//! confirm rows in an in-memory DB, ticks the worker, asserts state transitions
//! and that the configured stub mailer was driven the expected number of times.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use lettre::Message;

// Mirror the binary's module tree. Each integration test file is its own
// crate so we cannot `use blog_rs::*`; the `#[path = "..."]` trick keeps a
// single source of truth in src/.
#[path = "../src/config.rs"]
mod config;
#[path = "../src/embed.rs"]
mod embed;
#[path = "../src/error.rs"]
mod error;
#[path = "../src/mailer/mod.rs"]
mod mailer;
#[path = "../src/middleware/mod.rs"]
mod middleware;
#[path = "../src/routes/mod.rs"]
mod routes;
#[path = "../src/state.rs"]
mod state;
#[path = "../src/templates.rs"]
mod templates;
#[path = "../src/tokens.rs"]
mod tokens;
#[path = "../src/view.rs"]
mod view;
#[path = "../src/worker/mod.rs"]
mod worker;

use mailer::{MailError, MailerHandle, Transport};
use state::{AppState, SiteConfig};

struct OkMailer {
    count: AtomicUsize,
}

#[async_trait]
impl Transport for OkMailer {
    async fn send(&self, _: Message) -> Result<(), MailError> {
        self.count.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

struct AlwaysFails;

#[async_trait]
impl Transport for AlwaysFails {
    async fn send(&self, _: Message) -> Result<(), MailError> {
        Err(MailError::Smtp("nope".into()))
    }
}

async fn fresh_state(mailer: MailerHandle) -> AppState {
    let pool = db::test_support::fresh_pool().await;
    // The outbox.post_id column has an FK to posts(id). The confirm-email path
    // uses post_id = 0 as a synthetic marker, so we insert a placeholder
    // posts row with id=0 to satisfy the FK without otherwise polluting any
    // listings (status='draft', deleted_at set so admin queries hide it).
    let now = time::OffsetDateTime::now_utc().unix_timestamp();
    sqlx::query(
        "INSERT INTO users (id, email, password_hash, role, created_at)
         VALUES (1, '__placeholder__', '', 'admin', ?)
         ON CONFLICT(id) DO NOTHING",
    )
    .bind(now)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO posts (id, slug, title, status, author_id, updated_at, created_at,
                            body_md, body_html, deleted_at)
         VALUES (0, '__confirm_marker__', '', 'draft', 1, ?, ?, '', '', ?)",
    )
    .bind(now)
    .bind(now)
    .bind(now)
    .execute(&pool)
    .await
    .unwrap();
    AppState::new(pool, config::Config::default(), vec![0u8; 32])
        .with_mailer(mailer)
        .with_site(SiteConfig {
            base_url: "http://localhost".into(),
            site_title: "test".into(),
            admin_from: "test <noreply@localhost>".into(),
        })
}

#[tokio::test]
async fn tick_sends_pending_confirm_rows() {
    let ok = Arc::new(OkMailer {
        count: AtomicUsize::new(0),
    });
    let state = fresh_state(ok.clone()).await;

    for email in ["a@example.com", "b@example.com", "c@example.com"] {
        let (m, _) = db::members::signup(&state.pool, email).await.unwrap();
        db::members::enqueue_confirm(&state.pool, m.id).await.unwrap();
    }

    let n = worker::outbox::tick(&state, 10, 3).await;
    assert_eq!(n, 3);
    assert_eq!(ok.count.load(Ordering::SeqCst), 3);

    let sent: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM newsletter_outbox WHERE status='sent'")
        .fetch_one(&state.pool)
        .await
        .unwrap();
    assert_eq!(sent, 3);
}

#[tokio::test]
async fn tick_marks_failures_back_to_pending_until_max_then_dead() {
    let fail: MailerHandle = Arc::new(AlwaysFails);
    let state = fresh_state(fail).await;
    let (m, _) = db::members::signup(&state.pool, "x@example.com").await.unwrap();
    db::members::enqueue_confirm(&state.pool, m.id).await.unwrap();

    // Attempt 1: failure → row flips back to pending (attempts=1).
    let n = worker::outbox::tick(&state, 10, 2).await;
    assert_eq!(n, 1);
    let (status, attempts): (String, i64) =
        sqlx::query_as("SELECT status, attempts FROM newsletter_outbox WHERE member_id = ?")
            .bind(m.id)
            .fetch_one(&state.pool)
            .await
            .unwrap();
    assert_eq!(status, "pending");
    assert_eq!(attempts, 1);

    // Attempt 2: failure → attempts hits max → row goes to dead.
    let n = worker::outbox::tick(&state, 10, 2).await;
    assert_eq!(n, 1);
    let (status, attempts): (String, i64) =
        sqlx::query_as("SELECT status, attempts FROM newsletter_outbox WHERE member_id = ?")
            .bind(m.id)
            .fetch_one(&state.pool)
            .await
            .unwrap();
    assert_eq!(status, "dead");
    assert_eq!(attempts, 2);

    // Dead rows are not re-claimed: next tick is a no-op.
    let n = worker::outbox::tick(&state, 10, 2).await;
    assert_eq!(n, 0);
}

#[tokio::test]
async fn tick_with_empty_queue_is_a_noop() {
    let ok: MailerHandle = Arc::new(OkMailer {
        count: AtomicUsize::new(0),
    });
    let state = fresh_state(ok.clone()).await;
    let n = worker::outbox::tick(&state, 10, 3).await;
    assert_eq!(n, 0);
}
