//! Tag upsert + post-tag link queries. Tag rows are append-only; deletion is
//! manual via admin if ever needed.

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::DbError;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, PartialEq, Eq)]
pub struct Tag {
    pub id: i64,
    pub slug: String,
    pub name: String,
}

pub async fn upsert(pool: &SqlitePool, slug: &str, name: &str) -> Result<Tag, DbError> {
    sqlx::query("INSERT INTO tags (slug, name) VALUES (?, ?) ON CONFLICT(slug) DO UPDATE SET name = excluded.name")
        .bind(slug)
        .bind(name)
        .execute(pool)
        .await?;
    find_by_slug(pool, slug).await
}

pub async fn find_by_slug(pool: &SqlitePool, slug: &str) -> Result<Tag, DbError> {
    sqlx::query_as::<_, Tag>("SELECT id, slug, name FROM tags WHERE slug = ?")
        .bind(slug)
        .fetch_one(pool)
        .await
        .map_err(DbError::from_row)
}

pub async fn attach(pool: &SqlitePool, post_id: i64, tag_id: i64) -> Result<(), DbError> {
    sqlx::query("INSERT OR IGNORE INTO post_tags (post_id, tag_id) VALUES (?, ?)")
        .bind(post_id)
        .bind(tag_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn detach_all(pool: &SqlitePool, post_id: i64) -> Result<u64, DbError> {
    let r = sqlx::query("DELETE FROM post_tags WHERE post_id = ?")
        .bind(post_id)
        .execute(pool)
        .await?;
    Ok(r.rows_affected())
}

pub async fn list_for_post(pool: &SqlitePool, post_id: i64) -> Result<Vec<Tag>, DbError> {
    let rows = sqlx::query_as::<_, Tag>(
        "SELECT t.id, t.slug, t.name
         FROM tags t JOIN post_tags pt ON pt.tag_id = t.id
         WHERE pt.post_id = ?
         ORDER BY t.name ASC",
    )
    .bind(post_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::fresh_pool;

    #[tokio::test]
    async fn upsert_returns_stable_id() {
        let pool = fresh_pool().await;
        let a = upsert(&pool, "rust", "Rust").await.unwrap();
        let b = upsert(&pool, "rust", "Rust (updated)").await.unwrap();
        assert_eq!(a.id, b.id);
        assert_eq!(b.name, "Rust (updated)");
    }

    #[tokio::test]
    async fn find_by_slug_missing() {
        let pool = fresh_pool().await;
        let err = find_by_slug(&pool, "ghost").await.unwrap_err();
        assert!(matches!(err, DbError::NotFound));
    }
}
