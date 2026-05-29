//! Drains the `newsletter_outbox` table. One tick:
//!   1. claim up to BATCH pending rows (state → 'sending')
//!   2. for each, build the right Message and call mailer.send
//!   3. update state to 'sent' on success or back to 'pending' / 'dead' on err.
//!
//! At-most-once is guaranteed by the (post_id, member_id) unique constraint:
//! even if the worker crashes between mailer.send returning and mark_sent
//! committing, the row stays in `sending` (not re-claimed) and the human
//! operator can decide whether to re-enqueue.
//!
//! Poll interval and per-tick batch size are configured via `OUTBOX_POLL_INTERVAL`
//! (seconds) and `OUTBOX_BATCH`. `OUTBOX_MAX_ATTEMPTS` decides when a failing
//! row is parked in the dead-letter bucket.

use std::time::Duration;

use askama::Template;
use lettre::message::header::ContentType;
use lettre::Message;
use tokio_util::sync::CancellationToken;

use crate::state::AppState;
use crate::templates::{ConfirmEmail, PostEmail};
use crate::tokens::Purpose;
use db::outbox;

// Reachable only from main's `worker::spawn_all` (which itself is dead in
// integration test compilation units). The `tick` / `dispatch` pair is what
// integration tests exercise directly.
#[allow(dead_code)]
const DEFAULT_POLL: u64 = 5;
#[allow(dead_code)]
const DEFAULT_BATCH: i64 = 32;
#[allow(dead_code)]
const DEFAULT_MAX_ATTEMPTS: i64 = 5;
/// How long a row may sit in `sending` before the next tick rotates it back
/// to `pending`. The default of five minutes is comfortably longer than any
/// realistic single-SMTP-delivery timeout while still short enough to recover
/// from a crash within one human-noticeable polling cycle.
#[allow(dead_code)]
const DEFAULT_RECLAIM_AFTER: i64 = 300;

#[allow(dead_code)]
pub async fn run(state: AppState, shutdown: CancellationToken) {
    let poll = std::env::var("OUTBOX_POLL_INTERVAL")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(DEFAULT_POLL);
    let batch = std::env::var("OUTBOX_BATCH")
        .ok()
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(DEFAULT_BATCH);
    let max_attempts = std::env::var("OUTBOX_MAX_ATTEMPTS")
        .ok()
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(DEFAULT_MAX_ATTEMPTS);
    let reclaim_after = std::env::var("OUTBOX_RECLAIM_AFTER")
        .ok()
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(DEFAULT_RECLAIM_AFTER);

    tracing::info!(
        poll,
        batch,
        max_attempts,
        reclaim_after,
        "outbox worker starting"
    );

    loop {
        // Reclaim before claiming: rows whose previous worker died between
        // `mailer.send` returning Ok and `mark_sent` committing get rotated
        // back to `pending` so this tick can re-claim them.
        match outbox::reclaim_stale(&state.pool, reclaim_after).await {
            Ok(n) if n > 0 => tracing::warn!(reclaimed = n, "reclaimed stale outbox rows"),
            Ok(_) => {}
            Err(e) => tracing::error!(error = ?e, "reclaim_stale failed"),
        }

        tokio::select! {
            _ = shutdown.cancelled() => {
                tracing::info!("outbox worker shutting down");
                break;
            }
            _ = tick(&state, batch, max_attempts) => {
                tokio::select! {
                    _ = shutdown.cancelled() => {
                        tracing::info!("outbox worker shutting down");
                        break;
                    }
                    _ = tokio::time::sleep(Duration::from_secs(poll)) => {}
                }
            }
        }
    }
}

/// Run one batch. Returns the number of rows processed.
#[allow(dead_code)] // Exercised by some integration tests; others pull this
                    // module for the side-effect of compiling the worker tree.
pub async fn tick(state: &AppState, batch: i64, max_attempts: i64) -> usize {
    {
        // Record liveness before doing any work so a slow DB or claim error
        // still surfaces "worker is alive" to /readyz.
        let mut hb = state.worker_heartbeat.lock().await;
        *hb = Some(std::time::Instant::now());
    }
    let rows = match outbox::claim_pending(&state.pool, batch).await {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(error = ?e, "claim_pending failed");
            return 0;
        }
    };
    let count = rows.len();
    for row in rows {
        match dispatch(state, &row).await {
            Ok(()) => {
                if let Err(e) = outbox::mark_sent(&state.pool, row.id).await {
                    tracing::error!(error = ?e, id = row.id, "mark_sent failed");
                }
            }
            Err(e) => {
                let msg = e.to_string();
                tracing::warn!(error = %msg, id = row.id, "send failed");
                if let Err(e) = outbox::mark_failed(&state.pool, row.id, &msg, max_attempts).await {
                    tracing::error!(error = ?e, id = row.id, "mark_failed failed");
                }
            }
        }
    }
    count
}

#[allow(dead_code)]
type DynErr = Box<dyn std::error::Error + Send + Sync>;

#[allow(dead_code)]
async fn dispatch(state: &AppState, row: &outbox::OutboxRow) -> Result<(), DynErr> {
    let member = db::members::find_by_id(&state.pool, row.member_id).await?;
    let member_id_u32 = u32::try_from(member.id).map_err(|_| "member_id overflow u32")?;

    // post_id IS NULL marks a confirm-purpose row (see db::members::enqueue_confirm).
    let post_id = match row.post_id {
        None => {
            let token = state.tokens.issue(member_id_u32, Purpose::Confirm)?;
            let confirm_url = format!(
                "{}/confirm/{}",
                state.site.base_url.trim_end_matches('/'),
                token
            );
            let html = ConfirmEmail {
                site_title: &state.site.site_title,
                confirm_url,
                ttl_hours: (state.tokens.ttl() / 3600).max(1),
            }
            .render()?;
            let msg = Message::builder()
                .from(state.site.admin_from.parse()?)
                .to(member.email.parse()?)
                .subject(format!(
                    "Confirm your {} subscription",
                    state.site.site_title
                ))
                .header(ContentType::TEXT_HTML)
                .body(html)?;
            state.mailer.send(msg).await?;
            return Ok(());
        }
        Some(id) => id,
    };

    // Post dispatch: fetch the post, render the email, issue an unsubscribe
    // token, send. `find_by_id` returns a full Post row; we only use a handful
    // of fields, but reusing the existing accessor keeps the worker free of
    // bespoke SQL.
    let post = db::posts::find_by_id(&state.pool, post_id).await?;
    let unsub_token = state.tokens.issue(member_id_u32, Purpose::Unsubscribe)?;
    let post_url = format!(
        "{}/posts/{}",
        state.site.base_url.trim_end_matches('/'),
        post.slug
    );
    let unsubscribe_url = format!(
        "{}/unsubscribe/{}",
        state.site.base_url.trim_end_matches('/'),
        unsub_token
    );

    let html = PostEmail {
        site_title: &state.site.site_title,
        post_title: &post.title,
        subtitle: post.subtitle.as_deref(),
        excerpt: post.excerpt.as_deref(),
        post_url,
        unsubscribe_url,
    }
    .render()?;

    let msg = Message::builder()
        .from(state.site.admin_from.parse()?)
        .to(member.email.parse()?)
        .subject(post.title.clone())
        .header(ContentType::TEXT_HTML)
        .body(html)?;
    state.mailer.send(msg).await?;
    Ok(())
}
