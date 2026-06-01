//! Page CRUD queries. `body_html` is always set by the caller — the `content`
//! crate renders it before persistence (render-cache invariant).

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use time::OffsetDateTime;

use crate::DbError;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, PartialEq, Eq)]
pub struct Page {
    pub id: i64,
    pub slug: String,
    pub title: String,
    pub body_md: String,
    pub body_html: String,
    pub body_html_version: i64,
    pub toc_json: String,
    pub meta_json: Option<String>,
    pub status: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone)]
pub struct NewPage<'a> {
    pub slug: &'a str,
    pub title: &'a str,
    pub body_md: &'a str,
    pub body_html: &'a str,
    pub toc_json: &'a str,
    pub meta_json: Option<&'a str>,
    pub status: &'a str,
}

pub async fn create(pool: &SqlitePool, p: NewPage<'_>) -> Result<i64, DbError> {
    if !matches!(p.status, "draft" | "published") {
        return Err(DbError::Invalid(format!("bad status `{}`", p.status)));
    }
    let now = OffsetDateTime::now_utc().unix_timestamp();
    let res = sqlx::query(
        "INSERT INTO pages (slug, title, body_md, body_html, body_html_version,
                            toc_json, meta_json, status, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(p.slug)
    .bind(p.title)
    .bind(p.body_md)
    .bind(p.body_html)
    .bind(content::RENDER_VERSION as i64)
    .bind(p.toc_json)
    .bind(p.meta_json)
    .bind(p.status)
    .bind(now)
    .bind(now)
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

pub async fn find_by_slug(pool: &SqlitePool, slug: &str) -> Result<Page, DbError> {
    sqlx::query_as::<_, Page>("SELECT * FROM pages WHERE slug = ?")
        .bind(slug)
        .fetch_one(pool)
        .await
        .map_err(DbError::from_row)
}

pub async fn find_by_id(pool: &SqlitePool, id: i64) -> Result<Page, DbError> {
    sqlx::query_as::<_, Page>("SELECT * FROM pages WHERE id = ?")
        .bind(id)
        .fetch_one(pool)
        .await
        .map_err(DbError::from_row)
}

/// List all pages (for admin). Ordered by updated_at DESC.
pub async fn list_all(pool: &SqlitePool) -> Result<Vec<Page>, DbError> {
    let rows = sqlx::query_as::<_, Page>("SELECT * FROM pages ORDER BY updated_at DESC")
        .fetch_all(pool)
        .await?;
    Ok(rows)
}

#[derive(Debug, Clone, Default)]
pub struct PageUpdate {
    pub title: Option<String>,
    pub slug: Option<String>,
    pub body_md: Option<String>,
    pub body_html: Option<String>,
    pub toc_json: Option<String>,
    pub meta_json: Option<Option<String>>,
    pub status: Option<String>,
}

pub async fn update_fields(pool: &SqlitePool, id: i64, u: &PageUpdate) -> Result<(), DbError> {
    let now = OffsetDateTime::now_utc().unix_timestamp();
    let mut tx = pool.begin().await?;

    if let Some(v) = &u.title {
        sqlx::query("UPDATE pages SET title = ?, updated_at = ? WHERE id = ?")
            .bind(v)
            .bind(now)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    if let Some(v) = &u.slug {
        sqlx::query("UPDATE pages SET slug = ?, updated_at = ? WHERE id = ?")
            .bind(v)
            .bind(now)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    if let (Some(md), Some(html)) = (&u.body_md, &u.body_html) {
        let toc = u.toc_json.as_deref().unwrap_or("[]");
        sqlx::query(
            "UPDATE pages SET body_md = ?, body_html = ?, toc_json = ?,
                              body_html_version = ?, updated_at = ? WHERE id = ?",
        )
        .bind(md)
        .bind(html)
        .bind(toc)
        .bind(content::RENDER_VERSION as i64)
        .bind(now)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    }
    if let Some(opt) = &u.meta_json {
        sqlx::query("UPDATE pages SET meta_json = ?, updated_at = ? WHERE id = ?")
            .bind(opt.as_deref())
            .bind(now)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    if let Some(v) = &u.status {
        if !matches!(v.as_str(), "draft" | "published") {
            return Err(DbError::Invalid(format!("bad status `{v}`")));
        }
        sqlx::query("UPDATE pages SET status = ?, updated_at = ? WHERE id = ?")
            .bind(v)
            .bind(now)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }

    tx.commit().await?;
    Ok(())
}

pub async fn delete(pool: &SqlitePool, id: i64) -> Result<(), DbError> {
    sqlx::query("DELETE FROM pages WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Overwrite the rendered cache for a page (`body_html`, `toc_json`,
/// `body_html_version`). Used by the lazy regeneration path when a published
/// page is read with a stale `body_html_version`.
pub async fn update_rendered_cache(
    pool: &SqlitePool,
    id: i64,
    body_html: &str,
    toc_json: &str,
    version: i64,
) -> Result<(), DbError> {
    let now = OffsetDateTime::now_utc().unix_timestamp();
    sqlx::query(
        "UPDATE pages SET body_html = ?, toc_json = ?, body_html_version = ?, updated_at = ?
         WHERE id = ?",
    )
    .bind(body_html)
    .bind(toc_json)
    .bind(version)
    .bind(now)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::fresh_pool;

    async fn seed_page(pool: &SqlitePool, slug: &str, status: &str) -> i64 {
        create(
            pool,
            NewPage {
                slug,
                title: "Test Page",
                body_md: "# Hello\n\nBody text here.",
                body_html: "<h1>Hello</h1><p>Body text here.</p>",
                toc_json: "[]",
                meta_json: None,
                status,
            },
        )
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn create_then_find_round_trip() {
        let pool = fresh_pool().await;
        let id = seed_page(&pool, "about", "draft").await;
        let page = find_by_id(&pool, id).await.unwrap();
        assert_eq!(page.slug, "about");
        assert_eq!(page.status, "draft");
        assert_eq!(page.title, "Test Page");
        assert_eq!(page.body_html, "<h1>Hello</h1><p>Body text here.</p>");
    }

    #[tokio::test]
    async fn find_by_slug_round_trip() {
        let pool = fresh_pool().await;
        seed_page(&pool, "contact", "published").await;
        let page = find_by_slug(&pool, "contact").await.unwrap();
        assert_eq!(page.slug, "contact");
        assert_eq!(page.status, "published");
    }

    #[tokio::test]
    async fn find_by_slug_unknown_returns_not_found() {
        let pool = fresh_pool().await;
        let err = find_by_slug(&pool, "does-not-exist").await.unwrap_err();
        assert!(matches!(err, DbError::NotFound));
    }

    #[tokio::test]
    async fn bad_status_rejected() {
        let pool = fresh_pool().await;
        let err = create(
            &pool,
            NewPage {
                slug: "x",
                title: "X",
                body_md: "",
                body_html: "",
                toc_json: "[]",
                meta_json: None,
                status: "scheduled", // not valid for pages
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(err, DbError::Invalid(_)));
    }

    #[tokio::test]
    async fn duplicate_slug_conflicts() {
        let pool = fresh_pool().await;
        seed_page(&pool, "dup", "draft").await;
        let err = create(
            &pool,
            NewPage {
                slug: "dup",
                title: "Dup2",
                body_md: "",
                body_html: "",
                toc_json: "[]",
                meta_json: None,
                status: "draft",
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(err, DbError::Conflict(_)));
    }

    #[tokio::test]
    async fn list_all_returns_all_pages() {
        let pool = fresh_pool().await;
        seed_page(&pool, "p1", "draft").await;
        seed_page(&pool, "p2", "published").await;
        let pages = list_all(&pool).await.unwrap();
        assert_eq!(pages.len(), 2);
    }

    #[tokio::test]
    async fn update_rendered_cache_writes_version() {
        let pool = fresh_pool().await;
        let id = seed_page(&pool, "regen-page", "published").await;

        // Manually set version to 0 to simulate stale.
        sqlx::query("UPDATE pages SET body_html_version = 0 WHERE id = ?")
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();

        let md = "# New heading\n\nFresh content.";
        let out = content::render(md).expect("render failed");
        let toc_json = serde_json::to_string(&out.toc).unwrap();

        update_rendered_cache(
            &pool,
            id,
            &out.html,
            &toc_json,
            content::RENDER_VERSION as i64,
        )
        .await
        .unwrap();

        let row = find_by_id(&pool, id).await.unwrap();
        assert_eq!(row.body_html_version, content::RENDER_VERSION as i64);
        assert_eq!(row.body_html, out.html);
    }

    #[tokio::test]
    async fn update_fields_changes_title_and_status() {
        let pool = fresh_pool().await;
        let id = seed_page(&pool, "updateme", "draft").await;
        update_fields(
            &pool,
            id,
            &PageUpdate {
                title: Some("New Title".into()),
                status: Some("published".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let page = find_by_id(&pool, id).await.unwrap();
        assert_eq!(page.title, "New Title");
        assert_eq!(page.status, "published");
    }

    #[tokio::test]
    async fn delete_removes_page() {
        let pool = fresh_pool().await;
        let id = seed_page(&pool, "deleteme", "draft").await;
        delete(&pool, id).await.unwrap();
        let err = find_by_id(&pool, id).await.unwrap_err();
        assert!(matches!(err, DbError::NotFound));
    }
}
