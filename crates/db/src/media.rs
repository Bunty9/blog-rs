//! Media library queries. Files themselves live on disk (see
//! `blog-rs::state::SiteConfig::media_dir`); this module only owns the
//! metadata row. `filename` is the content-addressed name the file is
//! stored/served under; `original_name` is the user's upload name, kept for
//! display only.

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use time::OffsetDateTime;

use crate::DbError;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, PartialEq, Eq)]
pub struct Media {
    pub id: i64,
    pub filename: String,
    pub original_name: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub sha256: String,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub alt: String,
    pub created_at: i64,
}

#[derive(Debug, Clone)]
pub struct NewMedia<'a> {
    pub filename: &'a str,
    pub original_name: &'a str,
    pub content_type: &'a str,
    pub size_bytes: i64,
    pub sha256: &'a str,
    pub width: Option<i64>,
    pub height: Option<i64>,
}

/// Insert a new media row, deduping by `sha256`: if a row with the same
/// content hash already exists, that row is returned unchanged (the caller
/// should skip writing the file to disk in that case).
pub async fn insert(pool: &SqlitePool, m: NewMedia<'_>) -> Result<Media, DbError> {
    if let Some(existing) = find_by_sha256(pool, m.sha256).await? {
        return Ok(existing);
    }
    let now = OffsetDateTime::now_utc().unix_timestamp();
    let res = sqlx::query(
        "INSERT INTO media (filename, original_name, content_type, size_bytes, sha256, width, height, alt, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, '', ?)",
    )
    .bind(m.filename)
    .bind(m.original_name)
    .bind(m.content_type)
    .bind(m.size_bytes)
    .bind(m.sha256)
    .bind(m.width)
    .bind(m.height)
    .bind(now)
    .execute(pool)
    .await;

    match res {
        Ok(res) => find_by_id(pool, res.last_insert_rowid()).await,
        // Lost the race with a concurrent insert of the same content: fetch
        // whatever landed instead of erroring.
        Err(sqlx::Error::Database(db)) if db.is_unique_violation() => {
            find_by_sha256(pool, m.sha256)
                .await?
                .ok_or(DbError::NotFound)
        }
        Err(other) => Err(DbError::Sqlx(other)),
    }
}

pub async fn find_by_id(pool: &SqlitePool, id: i64) -> Result<Media, DbError> {
    sqlx::query_as::<_, Media>("SELECT * FROM media WHERE id = ?")
        .bind(id)
        .fetch_one(pool)
        .await
        .map_err(DbError::from_row)
}

pub async fn find_by_filename(pool: &SqlitePool, filename: &str) -> Result<Media, DbError> {
    sqlx::query_as::<_, Media>("SELECT * FROM media WHERE filename = ?")
        .bind(filename)
        .fetch_one(pool)
        .await
        .map_err(DbError::from_row)
}

pub async fn find_by_sha256(pool: &SqlitePool, sha256: &str) -> Result<Option<Media>, DbError> {
    let row = sqlx::query_as::<_, Media>("SELECT * FROM media WHERE sha256 = ?")
        .bind(sha256)
        .fetch_optional(pool)
        .await?;
    Ok(row)
}

/// List media, newest first, for the admin grid / picker.
pub async fn list(pool: &SqlitePool, limit: i64, offset: i64) -> Result<Vec<Media>, DbError> {
    let rows = sqlx::query_as::<_, Media>(
        "SELECT * FROM media ORDER BY created_at DESC, id DESC LIMIT ? OFFSET ?",
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn update_alt(pool: &SqlitePool, id: i64, alt: &str) -> Result<(), DbError> {
    let res = sqlx::query("UPDATE media SET alt = ? WHERE id = ?")
        .bind(alt)
        .bind(id)
        .execute(pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(DbError::NotFound);
    }
    Ok(())
}

pub async fn delete(pool: &SqlitePool, id: i64) -> Result<(), DbError> {
    let res = sqlx::query("DELETE FROM media WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(DbError::NotFound);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::fresh_pool;

    fn sample(sha: &str) -> NewMedia<'_> {
        NewMedia {
            filename: "abc0123456789def.png",
            original_name: "cat.png",
            content_type: "image/png",
            size_bytes: 42,
            sha256: sha,
            width: Some(10),
            height: Some(20),
        }
    }

    #[tokio::test]
    async fn insert_then_find_round_trip() {
        let pool = fresh_pool().await;
        let m = insert(&pool, sample("sha-a")).await.unwrap();
        assert_eq!(m.filename, "abc0123456789def.png");
        assert_eq!(m.alt, "");

        let by_id = find_by_id(&pool, m.id).await.unwrap();
        assert_eq!(by_id, m);

        let by_name = find_by_filename(&pool, "abc0123456789def.png")
            .await
            .unwrap();
        assert_eq!(by_name, m);
    }

    #[tokio::test]
    async fn insert_dedupes_by_sha256() {
        let pool = fresh_pool().await;
        let first = insert(&pool, sample("sha-dup")).await.unwrap();
        let second = insert(
            &pool,
            NewMedia {
                filename: "different-name.png",
                original_name: "other.png",
                ..sample("sha-dup")
            },
        )
        .await
        .unwrap();
        assert_eq!(first.id, second.id);
        assert_eq!(second.filename, "abc0123456789def.png");

        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM media")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count.0, 1);
    }

    #[tokio::test]
    async fn find_by_filename_unknown_returns_not_found() {
        let pool = fresh_pool().await;
        let err = find_by_filename(&pool, "nope.png").await.unwrap_err();
        assert!(matches!(err, DbError::NotFound));
    }

    #[tokio::test]
    async fn list_orders_newest_first() {
        let pool = fresh_pool().await;
        insert(&pool, sample("sha-1")).await.unwrap();
        insert(
            &pool,
            NewMedia {
                filename: "second.png",
                ..sample("sha-2")
            },
        )
        .await
        .unwrap();

        let rows = list(&pool, 60, 0).await.unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].sha256, "sha-2");
        assert_eq!(rows[1].sha256, "sha-1");
    }

    #[tokio::test]
    async fn update_alt_changes_value() {
        let pool = fresh_pool().await;
        let m = insert(&pool, sample("sha-alt")).await.unwrap();
        update_alt(&pool, m.id, "a happy cat").await.unwrap();
        let row = find_by_id(&pool, m.id).await.unwrap();
        assert_eq!(row.alt, "a happy cat");
    }

    #[tokio::test]
    async fn update_alt_unknown_id_not_found() {
        let pool = fresh_pool().await;
        let err = update_alt(&pool, 999, "x").await.unwrap_err();
        assert!(matches!(err, DbError::NotFound));
    }

    #[tokio::test]
    async fn delete_removes_row() {
        let pool = fresh_pool().await;
        let m = insert(&pool, sample("sha-del")).await.unwrap();
        delete(&pool, m.id).await.unwrap();
        let err = find_by_id(&pool, m.id).await.unwrap_err();
        assert!(matches!(err, DbError::NotFound));
    }

    #[tokio::test]
    async fn delete_unknown_id_not_found() {
        let pool = fresh_pool().await;
        let err = delete(&pool, 999).await.unwrap_err();
        assert!(matches!(err, DbError::NotFound));
    }
}
