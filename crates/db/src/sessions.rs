//! Server-side session rows. Tokens are produced by the `auth` crate; this
//! crate stores them. `expires_at` is a unix-seconds timestamp.

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use time::OffsetDateTime;

use crate::DbError;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, PartialEq, Eq)]
pub struct Session {
    pub token: String,
    pub user_id: i64,
    pub csrf_token: String,
    pub expires_at: i64,
    pub created_at: i64,
}

pub async fn create(
    pool: &SqlitePool,
    token: &str,
    user_id: i64,
    csrf_token: &str,
    expires_at: i64,
) -> Result<Session, DbError> {
    let now = OffsetDateTime::now_utc().unix_timestamp();
    sqlx::query(
        "INSERT INTO sessions (token, user_id, csrf_token, expires_at, created_at)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(token)
    .bind(user_id)
    .bind(csrf_token)
    .bind(expires_at)
    .bind(now)
    .execute(pool)
    .await?;

    Ok(Session {
        token: token.to_string(),
        user_id,
        csrf_token: csrf_token.to_string(),
        expires_at,
        created_at: now,
    })
}

/// Look up a session by token. Returns NotFound if missing or expired.
pub async fn find_active(pool: &SqlitePool, token: &str) -> Result<Session, DbError> {
    let now = OffsetDateTime::now_utc().unix_timestamp();
    sqlx::query_as::<_, Session>(
        "SELECT token, user_id, csrf_token, expires_at, created_at
         FROM sessions
         WHERE token = ? AND expires_at > ?",
    )
    .bind(token)
    .bind(now)
    .fetch_one(pool)
    .await
    .map_err(DbError::from_row)
}

pub async fn destroy(pool: &SqlitePool, token: &str) -> Result<u64, DbError> {
    let r = sqlx::query("DELETE FROM sessions WHERE token = ?")
        .bind(token)
        .execute(pool)
        .await?;
    Ok(r.rows_affected())
}

pub async fn destroy_all_for_user(pool: &SqlitePool, user_id: i64) -> Result<u64, DbError> {
    let r = sqlx::query("DELETE FROM sessions WHERE user_id = ?")
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(r.rows_affected())
}

/// Hard-delete every session whose expiry has passed.
pub async fn purge_expired(pool: &SqlitePool) -> Result<u64, DbError> {
    let now = OffsetDateTime::now_utc().unix_timestamp();
    let r = sqlx::query("DELETE FROM sessions WHERE expires_at <= ?")
        .bind(now)
        .execute(pool)
        .await?;
    Ok(r.rows_affected())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::fresh_pool;
    use crate::users;

    async fn seed_user(pool: &SqlitePool) -> i64 {
        users::bootstrap_admin(pool, "a@b.c", "hash").await.unwrap();
        users::find_by_email(pool, "a@b.c").await.unwrap().id
    }

    #[tokio::test]
    async fn round_trip_create_lookup_destroy() {
        let pool = fresh_pool().await;
        let uid = seed_user(&pool).await;
        let exp = OffsetDateTime::now_utc().unix_timestamp() + 3600;
        let s = create(&pool, "tok-1", uid, "csrf-1", exp).await.unwrap();
        let got = find_active(&pool, "tok-1").await.unwrap();
        assert_eq!(got, s);
        let n = destroy(&pool, "tok-1").await.unwrap();
        assert_eq!(n, 1);
        assert!(matches!(find_active(&pool, "tok-1").await, Err(DbError::NotFound)));
    }

    #[tokio::test]
    async fn expired_session_is_not_found() {
        let pool = fresh_pool().await;
        let uid = seed_user(&pool).await;
        let past = OffsetDateTime::now_utc().unix_timestamp() - 10;
        create(&pool, "old", uid, "csrf", past).await.unwrap();
        assert!(matches!(find_active(&pool, "old").await, Err(DbError::NotFound)));
    }

    #[tokio::test]
    async fn purge_drops_expired_only() {
        let pool = fresh_pool().await;
        let uid = seed_user(&pool).await;
        let now = OffsetDateTime::now_utc().unix_timestamp();
        create(&pool, "live", uid, "c1", now + 3600).await.unwrap();
        create(&pool, "dead", uid, "c2", now - 1).await.unwrap();
        let n = purge_expired(&pool).await.unwrap();
        assert_eq!(n, 1);
        assert!(find_active(&pool, "live").await.is_ok());
    }
}
