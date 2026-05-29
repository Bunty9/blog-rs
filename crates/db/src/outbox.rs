//! Newsletter outbox rows. Phase 1b just owns the data shape and atomic
//! state transitions; the SMTP worker lands in Phase 2.

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use time::OffsetDateTime;

use crate::DbError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutboxStatus {
    Pending,
    Sending,
    Sent,
    Failed,
    Dead,
}

impl OutboxStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Sending => "sending",
            Self::Sent => "sent",
            Self::Failed => "failed",
            Self::Dead => "dead",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, PartialEq, Eq)]
pub struct OutboxRow {
    pub id: i64,
    /// `None` marks a confirm-purpose row (no post yet exists); `Some(id)` is a
    /// per-post fan-out row. See migration 0007.
    pub post_id: Option<i64>,
    pub member_id: i64,
    pub status: String,
    pub attempts: i64,
    pub last_error: Option<String>,
    pub sent_at: Option<i64>,
    pub created_at: i64,
}

/// Enqueue one row per (post, member). Idempotent because of the UNIQUE
/// constraint on (post_id, member_id); duplicates are silently ignored.
pub async fn enqueue(pool: &SqlitePool, post_id: i64, member_id: i64) -> Result<bool, DbError> {
    let now = OffsetDateTime::now_utc().unix_timestamp();
    let r = sqlx::query(
        "INSERT OR IGNORE INTO newsletter_outbox (post_id, member_id, status, created_at)
         VALUES (?, ?, 'pending', ?)",
    )
    .bind(post_id)
    .bind(member_id)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(r.rows_affected() == 1)
}

/// Atomically claim up to `n` pending rows by flipping them to `sending`.
/// Returns the rows that were claimed.
pub async fn claim_pending(pool: &SqlitePool, n: i64) -> Result<Vec<OutboxRow>, DbError> {
    let mut tx = pool.begin().await?;
    let rows = sqlx::query_as::<_, OutboxRow>(
        "SELECT * FROM newsletter_outbox WHERE status = 'pending' ORDER BY created_at ASC LIMIT ?",
    )
    .bind(n)
    .fetch_all(&mut *tx)
    .await?;
    for r in &rows {
        sqlx::query("UPDATE newsletter_outbox SET status = 'sending' WHERE id = ?")
            .bind(r.id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(rows
        .into_iter()
        .map(|mut r| {
            r.status = "sending".into();
            r
        })
        .collect())
}

pub async fn mark_sent(pool: &SqlitePool, id: i64) -> Result<u64, DbError> {
    let now = OffsetDateTime::now_utc().unix_timestamp();
    let r = sqlx::query("UPDATE newsletter_outbox SET status = 'sent', sent_at = ? WHERE id = ?")
        .bind(now)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(r.rows_affected())
}

pub async fn mark_failed(
    pool: &SqlitePool,
    id: i64,
    error: &str,
    max_attempts: i64,
) -> Result<u64, DbError> {
    let r = sqlx::query(
        "UPDATE newsletter_outbox
         SET attempts = attempts + 1,
             last_error = ?,
             status = CASE WHEN attempts + 1 >= ? THEN 'dead' ELSE 'pending' END
         WHERE id = ?",
    )
    .bind(error)
    .bind(max_attempts)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(r.rows_affected())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::fresh_pool;
    use crate::{members, posts, users};

    async fn seed_post_member(pool: &SqlitePool) -> (i64, i64) {
        users::bootstrap_admin(pool, "a@b.c", "h").await.unwrap();
        let uid = users::find_by_email(pool, "a@b.c").await.unwrap().id;
        let pid = posts::create(
            pool,
            posts::NewPost {
                slug: "p",
                title: "P",
                subtitle: None,
                status: "draft",
                author_id: uid,
                excerpt: None,
                cover_image: None,
                body_md: "x",
                body_html: "x",
                meta_json: None,
            },
        )
        .await
        .unwrap();
        let mid = members::create_pending(pool, "m@x.y").await.unwrap();
        (pid, mid)
    }

    #[tokio::test]
    async fn enqueue_idempotent() {
        let pool = fresh_pool().await;
        let (pid, mid) = seed_post_member(&pool).await;
        assert!(enqueue(&pool, pid, mid).await.unwrap());
        assert!(!enqueue(&pool, pid, mid).await.unwrap());
    }

    #[tokio::test]
    async fn claim_then_mark_sent() {
        let pool = fresh_pool().await;
        let (pid, mid) = seed_post_member(&pool).await;
        enqueue(&pool, pid, mid).await.unwrap();
        let claimed = claim_pending(&pool, 10).await.unwrap();
        assert_eq!(claimed.len(), 1);
        assert_eq!(claimed[0].status, "sending");
        assert_eq!(mark_sent(&pool, claimed[0].id).await.unwrap(), 1);
    }

    #[tokio::test]
    async fn mark_failed_promotes_to_dead_after_threshold() {
        let pool = fresh_pool().await;
        let (pid, mid) = seed_post_member(&pool).await;
        enqueue(&pool, pid, mid).await.unwrap();
        let claimed = claim_pending(&pool, 10).await.unwrap();
        mark_failed(&pool, claimed[0].id, "boom", 1).await.unwrap();
        let (status,): (String,) =
            sqlx::query_as("SELECT status FROM newsletter_outbox WHERE id = ?")
                .bind(claimed[0].id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(status, "dead");
    }
}
