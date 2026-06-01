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

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct TagCount {
    pub slug: String,
    pub name: String,
    pub count: i64,
}

pub async fn list_with_counts(pool: &SqlitePool) -> Result<Vec<TagCount>, DbError> {
    let rows = sqlx::query_as::<_, TagCount>(
        "SELECT t.slug, t.name, COUNT(pt.post_id) AS count
         FROM tags t
         LEFT JOIN post_tags pt ON pt.tag_id = t.id
         GROUP BY t.id
         ORDER BY count DESC, t.name",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
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

    #[tokio::test]
    async fn list_with_counts_orders_by_use() {
        let pool = fresh_pool().await;

        // Seed a user (author_id FK)
        sqlx::query(
            "INSERT INTO users (id, email, password_hash, role, created_at) \
             VALUES (1, 'a@b', 'x', 'admin', 0)",
        )
        .execute(&pool)
        .await
        .unwrap();

        // Seed two tags
        let tag_a = upsert(&pool, "rust", "Rust").await.unwrap();
        let tag_b = upsert(&pool, "go", "Go").await.unwrap();

        // Helper: insert one published post and attach to a tag
        let attach_post = |slug: &'static str, title: &'static str, tag_id: i64| {
            let pool = pool.clone();
            async move {
                sqlx::query(
                    r#"INSERT INTO posts (slug, title, status, author_id, published_at, updated_at, created_at, body_md, body_html, meta_json, assets_json)
                       VALUES (?, ?, 'published', 1, 1700000000, 1700000000, 1700000000, '#x', '<h1>x</h1>', '{}', '[]')"#,
                )
                .bind(slug)
                .bind(title)
                .execute(&pool)
                .await
                .unwrap();

                let post_id: i64 = sqlx::query_scalar("SELECT id FROM posts WHERE slug = ?")
                    .bind(slug)
                    .fetch_one(&pool)
                    .await
                    .unwrap();

                attach(&pool, post_id, tag_id).await.unwrap();
            }
        };

        // Tag A gets 2 published posts, tag B gets 1
        attach_post("post-1", "Post 1", tag_a.id).await;
        attach_post("post-2", "Post 2", tag_a.id).await;
        attach_post("post-3", "Post 3", tag_b.id).await;

        let counts = list_with_counts(&pool).await.unwrap();

        // Must be ordered desc: Rust (2) before Go (1)
        assert_eq!(counts.len(), 2);
        assert_eq!(counts[0].slug, "rust");
        assert_eq!(counts[0].count, 2);
        assert_eq!(counts[1].slug, "go");
        assert_eq!(counts[1].count, 1);
    }
}
