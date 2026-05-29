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
    let r = sqlx::query("UPDATE members SET unsubscribed_at = ? WHERE id = ?")
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
        insert_fixture(&pool, "a@x.com", Some(1), None).await.unwrap();
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
        insert_fixture(&pool, "a@x.com", Some(1), None).await.unwrap();
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
