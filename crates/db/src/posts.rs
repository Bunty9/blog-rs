//! Post CRUD + list queries. `body_html` is always set by the caller - the
//! `content` crate renders it before persistence (spec §4.2 invariant).

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use time::OffsetDateTime;

use crate::DbError;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, PartialEq, Eq)]
pub struct Post {
    pub id: i64,
    pub slug: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub status: String,
    pub author_id: i64,
    pub published_at: Option<i64>,
    pub scheduled_for: Option<i64>,
    pub updated_at: i64,
    pub created_at: i64,
    pub excerpt: Option<String>,
    pub cover_image: Option<String>,
    pub reading_minutes: Option<i64>,
    pub body_md: String,
    pub body_html: String,
    pub meta_json: Option<String>,
}

#[derive(Debug, Clone)]
pub struct NewPost<'a> {
    pub slug: &'a str,
    pub title: &'a str,
    pub subtitle: Option<&'a str>,
    pub status: &'a str,
    pub author_id: i64,
    pub excerpt: Option<&'a str>,
    pub cover_image: Option<&'a str>,
    pub body_md: &'a str,
    pub body_html: &'a str,
    pub meta_json: Option<&'a str>,
}

pub async fn create(pool: &SqlitePool, p: NewPost<'_>) -> Result<i64, DbError> {
    if !matches!(p.status, "draft" | "published" | "scheduled") {
        return Err(DbError::Invalid(format!("bad status `{}`", p.status)));
    }
    let now = OffsetDateTime::now_utc().unix_timestamp();
    let res = sqlx::query(
        "INSERT INTO posts (slug, title, subtitle, status, author_id, updated_at, created_at,
                            excerpt, cover_image, body_md, body_html, meta_json)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(p.slug)
    .bind(p.title)
    .bind(p.subtitle)
    .bind(p.status)
    .bind(p.author_id)
    .bind(now)
    .bind(now)
    .bind(p.excerpt)
    .bind(p.cover_image)
    .bind(p.body_md)
    .bind(p.body_html)
    .bind(p.meta_json)
    .execute(pool)
    .await
    .map_err(|e| match e {
        sqlx::Error::Database(db) if db.is_unique_violation() => {
            DbError::Conflict(format!("slug `{}` already exists", p.slug))
        }
        other => DbError::Sqlx(other),
    })?;
    Ok(res.last_insert_rowid())
}

pub async fn find_by_slug(pool: &SqlitePool, slug: &str) -> Result<Post, DbError> {
    sqlx::query_as::<_, Post>("SELECT * FROM posts WHERE slug = ?")
        .bind(slug)
        .fetch_one(pool)
        .await
        .map_err(DbError::from_row)
}

pub async fn find_by_id(pool: &SqlitePool, id: i64) -> Result<Post, DbError> {
    sqlx::query_as::<_, Post>("SELECT * FROM posts WHERE id = ?")
        .bind(id)
        .fetch_one(pool)
        .await
        .map_err(DbError::from_row)
}

pub async fn list_published(
    pool: &SqlitePool,
    limit: i64,
    offset: i64,
) -> Result<Vec<Post>, DbError> {
    let rows = sqlx::query_as::<_, Post>(
        "SELECT * FROM posts
         WHERE status = 'published'
         ORDER BY published_at DESC
         LIMIT ? OFFSET ?",
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::fresh_pool;
    use crate::users;

    async fn seed_author(pool: &SqlitePool) -> i64 {
        users::bootstrap_admin(pool, "a@b.c", "h").await.unwrap();
        users::find_by_email(pool, "a@b.c").await.unwrap().id
    }

    #[tokio::test]
    async fn create_then_lookup_round_trip() {
        let pool = fresh_pool().await;
        let uid = seed_author(&pool).await;
        let id = create(
            &pool,
            NewPost {
                slug: "hello",
                title: "Hello",
                subtitle: None,
                status: "draft",
                author_id: uid,
                excerpt: Some("ex"),
                cover_image: None,
                body_md: "# hi",
                body_html: "<h1>hi</h1>",
                meta_json: None,
            },
        )
        .await
        .unwrap();
        let got = find_by_id(&pool, id).await.unwrap();
        assert_eq!(got.slug, "hello");
        assert_eq!(got.status, "draft");
        assert_eq!(got.body_html, "<h1>hi</h1>");
    }

    #[tokio::test]
    async fn duplicate_slug_conflicts() {
        let pool = fresh_pool().await;
        let uid = seed_author(&pool).await;
        let mk = || NewPost {
            slug: "dup",
            title: "T",
            subtitle: None,
            status: "draft",
            author_id: uid,
            excerpt: None,
            cover_image: None,
            body_md: "x",
            body_html: "x",
            meta_json: None,
        };
        create(&pool, mk()).await.unwrap();
        let err = create(&pool, mk()).await.unwrap_err();
        assert!(matches!(err, DbError::Conflict(_)));
    }

    #[tokio::test]
    async fn bad_status_rejected() {
        let pool = fresh_pool().await;
        let uid = seed_author(&pool).await;
        let err = create(
            &pool,
            NewPost {
                slug: "x",
                title: "T",
                subtitle: None,
                status: "garbage",
                author_id: uid,
                excerpt: None,
                cover_image: None,
                body_md: "x",
                body_html: "x",
                meta_json: None,
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(err, DbError::Invalid(_)));
    }
}
