//! Admin user queries. The first row in this table is the seeded admin from
//! BLOG_ADMIN_EMAIL / BLOG_ADMIN_PASSWORD on first boot (spec §7).

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use time::OffsetDateTime;

use crate::DbError;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, PartialEq, Eq)]
pub struct User {
    pub id: i64,
    pub email: String,
    pub password_hash: String,
    pub role: String,
    pub created_at: i64,
}

/// Insert an admin if none exists yet. Returns `true` if a row was inserted.
/// Idempotent: no-op when any user is already present.
pub async fn bootstrap_admin(
    pool: &SqlitePool,
    email: &str,
    password_hash: &str,
) -> Result<bool, DbError> {
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
        .fetch_one(pool)
        .await?;
    if count.0 > 0 {
        return Ok(false);
    }
    let now = OffsetDateTime::now_utc().unix_timestamp();
    sqlx::query(
        "INSERT INTO users (email, password_hash, role, created_at) VALUES (?, ?, 'admin', ?)",
    )
    .bind(email)
    .bind(password_hash)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(true)
}

pub async fn find_by_email(pool: &SqlitePool, email: &str) -> Result<User, DbError> {
    sqlx::query_as::<_, User>(
        "SELECT id, email, password_hash, role, created_at FROM users WHERE email = ?",
    )
    .bind(email)
    .fetch_one(pool)
    .await
    .map_err(DbError::from_row)
}

pub async fn find_by_id(pool: &SqlitePool, id: i64) -> Result<User, DbError> {
    sqlx::query_as::<_, User>(
        "SELECT id, email, password_hash, role, created_at FROM users WHERE id = ?",
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .map_err(DbError::from_row)
}

pub async fn count(pool: &SqlitePool) -> Result<i64, DbError> {
    let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
        .fetch_one(pool)
        .await?;
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::fresh_pool;

    #[tokio::test]
    async fn bootstrap_inserts_then_noops() {
        let pool = fresh_pool().await;
        assert!(bootstrap_admin(&pool, "a@b.c", "hash1").await.unwrap());
        assert!(!bootstrap_admin(&pool, "x@y.z", "hash2").await.unwrap());
        let u = find_by_email(&pool, "a@b.c").await.unwrap();
        assert_eq!(u.password_hash, "hash1");
        assert_eq!(u.role, "admin");
    }

    #[tokio::test]
    async fn find_by_email_missing() {
        let pool = fresh_pool().await;
        let err = find_by_email(&pool, "ghost@example.com").await.unwrap_err();
        assert!(matches!(err, DbError::NotFound));
    }

    #[tokio::test]
    async fn unique_email_enforced() {
        let pool = fresh_pool().await;
        bootstrap_admin(&pool, "a@b.c", "h").await.unwrap();
        let now = time::OffsetDateTime::now_utc().unix_timestamp();
        let err = sqlx::query(
            "INSERT INTO users (email, password_hash, role, created_at) VALUES (?, ?, 'admin', ?)",
        )
        .bind("a@b.c")
        .bind("h2")
        .bind(now)
        .execute(&pool)
        .await
        .unwrap_err();
        assert!(matches!(err, sqlx::Error::Database(_)));
    }
}
