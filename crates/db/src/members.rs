//! Free-member subscribers. Only minimal scaffolding here; the public signup
//! flow lands in Phase 1d. We still need create/confirm/unsubscribe queries
//! so 1c reader pages can show subscriber counts and 1d can wire them up.

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use time::OffsetDateTime;

use crate::DbError;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, PartialEq, Eq)]
pub struct Member {
    pub id: i64,
    pub email: String,
    pub confirmed_at: Option<i64>,
    pub unsubscribed_at: Option<i64>,
    pub created_at: i64,
}

/// Insert a pending member. Returns the newly-inserted id, or NotFound-mapped
/// Conflict if email already exists.
pub async fn create_pending(pool: &SqlitePool, email: &str) -> Result<i64, DbError> {
    let now = OffsetDateTime::now_utc().unix_timestamp();
    let res = sqlx::query("INSERT INTO members (email, created_at) VALUES (?, ?)")
        .bind(email)
        .bind(now)
        .execute(pool)
        .await
        .map_err(|e| match e {
            sqlx::Error::Database(db) if db.is_unique_violation() => {
                DbError::Conflict(format!("email `{email}` already subscribed"))
            }
            other => DbError::Sqlx(other),
        })?;
    Ok(res.last_insert_rowid())
}

pub async fn confirm(pool: &SqlitePool, id: i64) -> Result<u64, DbError> {
    let now = OffsetDateTime::now_utc().unix_timestamp();
    let r =
        sqlx::query("UPDATE members SET confirmed_at = ? WHERE id = ? AND confirmed_at IS NULL")
            .bind(now)
            .bind(id)
            .execute(pool)
            .await?;
    Ok(r.rows_affected())
}

pub async fn unsubscribe(pool: &SqlitePool, id: i64) -> Result<u64, DbError> {
    let now = OffsetDateTime::now_utc().unix_timestamp();
    // COALESCE keeps the first unsubscribe timestamp on a repeated call so the
    // operation is truly idempotent (re-clicking the unsubscribe link does not
    // bump the audit trail forward).
    let r = sqlx::query(
        "UPDATE members SET unsubscribed_at = COALESCE(unsubscribed_at, ?) WHERE id = ?",
    )
    .bind(now)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(r.rows_affected())
}

pub async fn find_by_id(pool: &SqlitePool, id: i64) -> Result<Member, DbError> {
    sqlx::query_as::<_, Member>("SELECT * FROM members WHERE id = ?")
        .bind(id)
        .fetch_one(pool)
        .await
        .map_err(DbError::from_row)
}

pub async fn count_active(pool: &SqlitePool) -> Result<i64, DbError> {
    let (n,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM members WHERE confirmed_at IS NOT NULL AND unsubscribed_at IS NULL",
    )
    .fetch_one(pool)
    .await?;
    Ok(n)
}

// ---------------------------------------------------------------------------
// Admin queries (Plan 1d)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct AdminMemberRow {
    pub id: i64,
    pub email: String,
    pub confirmed_at: Option<i64>,
    pub unsubscribed_at: Option<i64>,
    pub created_at: i64,
}

pub async fn list_admin(pool: &SqlitePool, limit: i64) -> Result<Vec<AdminMemberRow>, DbError> {
    let rows = sqlx::query_as::<_, (i64, String, Option<i64>, Option<i64>, i64)>(
        "SELECT id, email, confirmed_at, unsubscribed_at, created_at
         FROM members ORDER BY created_at DESC LIMIT ?",
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(
            |(id, email, confirmed_at, unsubscribed_at, created_at)| AdminMemberRow {
                id,
                email,
                confirmed_at,
                unsubscribed_at,
                created_at,
            },
        )
        .collect())
}

pub async fn count_all(pool: &SqlitePool) -> Result<(i64, i64, i64), DbError> {
    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM members")
        .fetch_one(pool)
        .await?;
    let confirmed: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM members WHERE confirmed_at IS NOT NULL AND unsubscribed_at IS NULL",
    )
    .fetch_one(pool)
    .await?;
    let unsubscribed: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM members WHERE unsubscribed_at IS NOT NULL")
            .fetch_one(pool)
            .await?;
    Ok((total, confirmed, unsubscribed))
}

/// Snapshot every member email + status + created timestamp for CSV export.
pub async fn export_all(pool: &SqlitePool) -> Result<Vec<(String, &'static str, i64)>, DbError> {
    let rows = sqlx::query_as::<_, (String, Option<i64>, Option<i64>, i64)>(
        "SELECT email, confirmed_at, unsubscribed_at, created_at FROM members ORDER BY created_at ASC",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(email, conf, unsub, created)| {
            let status: &'static str = match (conf, unsub) {
                (_, Some(_)) => "unsubscribed",
                (Some(_), None) => "confirmed",
                (None, None) => "pending",
            };
            (email, status, created)
        })
        .collect())
}

// ---------------------------------------------------------------------------
// Phase 1e: signup + outbox helpers
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignupOutcome {
    /// Email was new; row inserted.
    Created,
    /// Email already existed and is still pending confirmation.
    AlreadyPending,
    /// Email already confirmed and active.
    AlreadyConfirmed,
    /// Email exists but is currently unsubscribed; resubscribing.
    Resubscribed,
}

/// Insert-or-resurrect member. Idempotent: returns an outcome the caller can
/// use to decide whether to enqueue a new confirm email. Email is normalised
/// to lowercase + trimmed before hitting the unique index.
pub async fn signup(pool: &SqlitePool, email: &str) -> Result<(Member, SignupOutcome), DbError> {
    let email = email.trim().to_ascii_lowercase();
    let now = OffsetDateTime::now_utc().unix_timestamp();

    if let Some(existing) = sqlx::query_as::<_, Member>(
        "SELECT id, email, confirmed_at, unsubscribed_at, created_at FROM members WHERE email = ?",
    )
    .bind(&email)
    .fetch_optional(pool)
    .await?
    {
        let outcome = match (existing.confirmed_at, existing.unsubscribed_at) {
            (Some(_), Some(_)) => {
                sqlx::query("UPDATE members SET unsubscribed_at = NULL WHERE id = ?")
                    .bind(existing.id)
                    .execute(pool)
                    .await?;
                SignupOutcome::Resubscribed
            }
            (Some(_), None) => SignupOutcome::AlreadyConfirmed,
            (None, _) => SignupOutcome::AlreadyPending,
        };
        let m = find_by_id(pool, existing.id).await?;
        return Ok((m, outcome));
    }

    let id = sqlx::query(
        "INSERT INTO members(email, confirmed_at, unsubscribed_at, created_at)
         VALUES (?, NULL, NULL, ?)",
    )
    .bind(&email)
    .bind(now)
    .execute(pool)
    .await?
    .last_insert_rowid();

    Ok((find_by_id(pool, id).await?, SignupOutcome::Created))
}

pub async fn change_email(pool: &SqlitePool, id: i64, new_email: &str) -> Result<(), DbError> {
    let normalized = new_email.trim().to_ascii_lowercase();
    sqlx::query("UPDATE members SET email = ? WHERE id = ?")
        .bind(normalized)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Enqueue confirm-email rows. Confirm-purpose rows carry `post_id = NULL`
/// since no real post exists yet (migration 0007 widened the column to allow
/// this). The worker treats NULL as the confirm slot. Post fan-out should use
/// `crate::outbox::enqueue` once a real post is published.
pub async fn enqueue_confirm(pool: &SqlitePool, member_id: i64) -> Result<(), DbError> {
    let now = OffsetDateTime::now_utc().unix_timestamp();
    sqlx::query(
        "INSERT OR IGNORE INTO newsletter_outbox(post_id, member_id, status, attempts, created_at)
         VALUES (NULL, ?, 'pending', 0, ?)",
    )
    .bind(member_id)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(())
}

/// Fan-out a published post to every confirmed, non-unsubscribed member.
/// Idempotent on (post_id, member_id).
pub async fn enqueue_post_to_all_confirmed(
    pool: &SqlitePool,
    post_id: i64,
) -> Result<u64, DbError> {
    let now = OffsetDateTime::now_utc().unix_timestamp();
    let res = sqlx::query(
        "INSERT OR IGNORE INTO newsletter_outbox(post_id, member_id, status, attempts, created_at)
         SELECT ?, id, 'pending', 0, ?
         FROM members
         WHERE confirmed_at IS NOT NULL AND unsubscribed_at IS NULL",
    )
    .bind(post_id)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(res.rows_affected())
}

#[cfg(any(test, feature = "test-helpers"))]
pub async fn insert_fixture(
    pool: &SqlitePool,
    email: &str,
    confirmed_at: Option<i64>,
    unsubscribed_at: Option<i64>,
) -> Result<i64, DbError> {
    let now = OffsetDateTime::now_utc().unix_timestamp();
    sqlx::query(
        "INSERT INTO members (email, confirmed_at, unsubscribed_at, created_at)
         VALUES (?, ?, ?, ?)",
    )
    .bind(email)
    .bind(confirmed_at)
    .bind(unsubscribed_at)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(sqlx::query_scalar("SELECT id FROM members WHERE email = ?")
        .bind(email)
        .fetch_one(pool)
        .await?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::fresh_pool;

    #[tokio::test]
    async fn create_confirm_unsubscribe_flow() {
        let pool = fresh_pool().await;
        let id = create_pending(&pool, "x@y.z").await.unwrap();
        assert_eq!(confirm(&pool, id).await.unwrap(), 1);
        assert_eq!(count_active(&pool).await.unwrap(), 1);
        assert_eq!(unsubscribe(&pool, id).await.unwrap(), 1);
        assert_eq!(count_active(&pool).await.unwrap(), 0);
    }

    #[tokio::test]
    async fn duplicate_email_conflicts() {
        let pool = fresh_pool().await;
        create_pending(&pool, "x@y.z").await.unwrap();
        let err = create_pending(&pool, "x@y.z").await.unwrap_err();
        assert!(matches!(err, DbError::Conflict(_)));
    }

    #[tokio::test]
    async fn unsubscribe_is_truly_idempotent() {
        let pool = fresh_pool().await;
        let id = create_pending(&pool, "x@y.z").await.unwrap();
        confirm(&pool, id).await.unwrap();

        unsubscribe(&pool, id).await.unwrap();
        let first = find_by_id(&pool, id).await.unwrap().unsubscribed_at;
        assert!(
            first.is_some(),
            "first unsubscribe should stamp a timestamp"
        );

        // Sleep just long enough that the second call would observe a strictly
        // greater `now`, then re-run. COALESCE should keep the original value.
        tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
        unsubscribe(&pool, id).await.unwrap();
        let second = find_by_id(&pool, id).await.unwrap().unsubscribed_at;
        assert_eq!(
            second, first,
            "second unsubscribe must not overwrite original timestamp"
        );
    }
}

#[cfg(test)]
mod admin_tests {
    use super::*;
    use crate::test_support::fresh_pool;

    #[tokio::test]
    async fn list_admin_orders_by_created_at_desc() {
        let pool = fresh_pool().await;
        insert_fixture(&pool, "old@x.com", Some(1000), None)
            .await
            .unwrap();
        sqlx::query("UPDATE members SET created_at = 1000 WHERE email='old@x.com'")
            .execute(&pool)
            .await
            .unwrap();
        insert_fixture(&pool, "new@x.com", Some(2000), None)
            .await
            .unwrap();
        sqlx::query("UPDATE members SET created_at = 2000 WHERE email='new@x.com'")
            .execute(&pool)
            .await
            .unwrap();
        let rows = list_admin(&pool, 10).await.unwrap();
        assert_eq!(rows[0].email, "new@x.com");
        assert_eq!(rows[1].email, "old@x.com");
    }

    #[tokio::test]
    async fn count_buckets() {
        let pool = fresh_pool().await;
        insert_fixture(&pool, "a@x.com", Some(1), None)
            .await
            .unwrap();
        insert_fixture(&pool, "b@x.com", Some(1), Some(2))
            .await
            .unwrap();
        insert_fixture(&pool, "c@x.com", None, None).await.unwrap();
        let (total, confirmed, unsub) = count_all(&pool).await.unwrap();
        assert_eq!(total, 3);
        assert_eq!(confirmed, 1);
        assert_eq!(unsub, 1);
    }

    #[tokio::test]
    async fn export_all_categorises_status() {
        let pool = fresh_pool().await;
        insert_fixture(&pool, "a@x.com", Some(1), None)
            .await
            .unwrap();
        insert_fixture(&pool, "b@x.com", Some(1), Some(2))
            .await
            .unwrap();
        insert_fixture(&pool, "c@x.com", None, None).await.unwrap();
        let rows = export_all(&pool).await.unwrap();
        let statuses: std::collections::HashMap<_, _> =
            rows.iter().map(|(e, s, _)| (e.as_str(), *s)).collect();
        assert_eq!(statuses["a@x.com"], "confirmed");
        assert_eq!(statuses["b@x.com"], "unsubscribed");
        assert_eq!(statuses["c@x.com"], "pending");
    }
}

#[cfg(test)]
mod signup_tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    // Inline schema slice mirrors the post-0007 production shape: `post_id` is
    // nullable so `enqueue_confirm` can write NULL for confirm-purpose rows.
    async fn pool() -> SqlitePool {
        let p = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query(
            "CREATE TABLE members (
                id INTEGER PRIMARY KEY,
                email TEXT NOT NULL UNIQUE,
                confirmed_at INTEGER,
                unsubscribed_at INTEGER,
                created_at INTEGER NOT NULL
             );",
        )
        .execute(&p)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TABLE newsletter_outbox (
                id INTEGER PRIMARY KEY,
                post_id INTEGER,
                member_id INTEGER NOT NULL,
                status TEXT NOT NULL,
                attempts INTEGER NOT NULL DEFAULT 0,
                last_error TEXT,
                sent_at INTEGER,
                created_at INTEGER NOT NULL,
                UNIQUE(post_id, member_id)
             );",
        )
        .execute(&p)
        .await
        .unwrap();
        sqlx::query(
            "CREATE UNIQUE INDEX outbox_confirm_unique_idx
                ON newsletter_outbox(member_id)
                WHERE post_id IS NULL;",
        )
        .execute(&p)
        .await
        .unwrap();
        p
    }

    #[tokio::test]
    async fn signup_inserts_then_idempotent() {
        let p = pool().await;
        let (m1, o1) = signup(&p, "x@example.com").await.unwrap();
        assert_eq!(o1, SignupOutcome::Created);
        let (m2, o2) = signup(&p, "X@Example.com").await.unwrap();
        assert_eq!(o2, SignupOutcome::AlreadyPending);
        assert_eq!(m1.id, m2.id);
    }

    #[tokio::test]
    async fn confirm_then_unsubscribe_then_resub() {
        let p = pool().await;
        let (m, _) = signup(&p, "a@example.com").await.unwrap();
        confirm(&p, m.id).await.unwrap();
        let m2 = find_by_id(&p, m.id).await.unwrap();
        assert!(m2.confirmed_at.is_some());
        unsubscribe(&p, m.id).await.unwrap();
        let m3 = find_by_id(&p, m.id).await.unwrap();
        assert!(m3.unsubscribed_at.is_some());
        let (_, outcome) = signup(&p, "a@example.com").await.unwrap();
        assert_eq!(outcome, SignupOutcome::Resubscribed);
        let m4 = find_by_id(&p, m.id).await.unwrap();
        assert!(m4.unsubscribed_at.is_none());
    }

    #[tokio::test]
    async fn enqueue_confirm_is_idempotent() {
        let p = pool().await;
        let (m, _) = signup(&p, "a@example.com").await.unwrap();
        enqueue_confirm(&p, m.id).await.unwrap();
        enqueue_confirm(&p, m.id).await.unwrap();
        let n: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM newsletter_outbox WHERE member_id = ?")
                .bind(m.id)
                .fetch_one(&p)
                .await
                .unwrap();
        assert_eq!(n, 1);
    }

    #[tokio::test]
    async fn change_email_updates_normalised() {
        let p = pool().await;
        let (m, _) = signup(&p, "a@example.com").await.unwrap();
        change_email(&p, m.id, "  B@EXAMPLE.com  ").await.unwrap();
        let m2 = find_by_id(&p, m.id).await.unwrap();
        assert_eq!(m2.email, "b@example.com");
    }

    #[tokio::test]
    async fn enqueue_post_to_all_confirmed_skips_pending_and_unsubscribed() {
        let p = pool().await;
        let (a, _) = signup(&p, "a@example.com").await.unwrap();
        confirm(&p, a.id).await.unwrap();
        let (b, _) = signup(&p, "b@example.com").await.unwrap();
        confirm(&p, b.id).await.unwrap();
        unsubscribe(&p, b.id).await.unwrap();
        let (_c, _) = signup(&p, "c@example.com").await.unwrap(); // pending
        let n = enqueue_post_to_all_confirmed(&p, 42).await.unwrap();
        assert_eq!(n, 1);
    }
}
