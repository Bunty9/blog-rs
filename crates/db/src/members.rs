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
