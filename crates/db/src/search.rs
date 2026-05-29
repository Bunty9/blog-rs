//! Full-text search against the `posts_fts` virtual table. Phase 1c surfaces
//! this via /search; here we ship the query helper and a trigger smoke test.

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::DbError;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, PartialEq)]
pub struct Hit {
    pub id: i64,
    pub slug: String,
    pub title: String,
    pub excerpt: Option<String>,
    pub rank: f64,
}

/// Run an FTS5 MATCH query over published posts only. `query` is passed
/// verbatim; the caller is expected to sanitise input (or wrap in quotes).
pub async fn search(pool: &SqlitePool, query: &str, limit: i64) -> Result<Vec<Hit>, DbError> {
    let rows = sqlx::query_as::<_, Hit>(
        "SELECT p.id, p.slug, p.title, p.excerpt, bm25(posts_fts) AS rank
         FROM posts_fts
         JOIN posts p ON p.id = posts_fts.rowid
         WHERE posts_fts MATCH ? AND p.status = 'published'
         ORDER BY rank
         LIMIT ?",
    )
    .bind(query)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::posts::{self, NewPost};
    use crate::test_support::fresh_pool;
    use crate::users;
    use time::OffsetDateTime;

    async fn seed_published(pool: &SqlitePool, slug: &str, title: &str, body: &str) {
        users::bootstrap_admin(pool, "a@b.c", "h").await.ok();
        let uid = users::find_by_email(pool, "a@b.c").await.unwrap().id;
        let id = posts::create(
            pool,
            NewPost {
                slug,
                title,
                subtitle: None,
                status: "published",
                author_id: uid,
                excerpt: Some("ex"),
                cover_image: None,
                body_md: body,
                body_html: body,
                meta_json: None,
            },
        )
        .await
        .unwrap();
        let now = OffsetDateTime::now_utc().unix_timestamp();
        sqlx::query("UPDATE posts SET published_at = ? WHERE id = ?")
            .bind(now)
            .bind(id)
            .execute(pool)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn fts_finds_inserted_post() {
        let pool = fresh_pool().await;
        seed_published(&pool, "rust", "Rust embedded", "Cortex M4 boot").await;
        seed_published(&pool, "rdb", "Database internals", "B-tree pages").await;

        let hits = search(&pool, "cortex", 10).await.unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].slug, "rust");
    }

    #[tokio::test]
    async fn update_trigger_keeps_index_fresh() {
        let pool = fresh_pool().await;
        seed_published(&pool, "p", "Original title", "body").await;
        sqlx::query("UPDATE posts SET title = 'Mutated title' WHERE slug = 'p'")
            .execute(&pool)
            .await
            .unwrap();

        let hits = search(&pool, "mutated", 10).await.unwrap();
        assert_eq!(hits.len(), 1);
        let stale = search(&pool, "original", 10).await.unwrap();
        assert_eq!(stale.len(), 0);
    }
}
