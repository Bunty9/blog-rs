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

const DEFAULT_POLL: u64 = 5;
const DEFAULT_BATCH: i64 = 32;
const DEFAULT_MAX_ATTEMPTS: i64 = 5;

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

    tracing::info!(poll, batch, max_attempts, "outbox worker starting");

    loop {
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
pub async fn tick(state: &AppState, batch: i64, max_attempts: i64) -> usize {
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
                if let Err(e) =
                    outbox::mark_failed(&state.pool, row.id, &msg, max_attempts).await
                {
                    tracing::error!(error = ?e, id = row.id, "mark_failed failed");
                }
            }
        }
    }
    count
}

type DynErr = Box<dyn std::error::Error + Send + Sync>;

async fn dispatch(state: &AppState, row: &outbox::OutboxRow) -> Result<(), DynErr> {
    let member = db::members::find_by_id(&state.pool, row.member_id).await?;
    let member_id_u32 = u32::try_from(member.id).map_err(|_| "member_id overflow u32")?;

    // post_id = 0 is the synthetic confirm slot (see db::members::enqueue_confirm).
    if row.post_id == 0 {
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
            .subject(format!("Confirm your {} subscription", state.site.site_title))
            .header(ContentType::TEXT_HTML)
            .body(html)?;
        state.mailer.send(msg).await?;
        return Ok(());
    }

    // Post dispatch: fetch the post, render the email, issue an unsubscribe
    // token, send. `find_by_id` returns a full Post row; we only use a handful
    // of fields, but reusing the existing accessor keeps the worker free of
    // bespoke SQL.
    let post = db::posts::find_by_id(&state.pool, row.post_id).await?;
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
