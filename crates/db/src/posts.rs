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
    pub assets_json: String,
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
                            excerpt, cover_image, body_md, body_html, body_html_version, meta_json)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
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
    .bind(content::RENDER_VERSION as i64)
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

use content::AssetManifest;

impl Post {
    /// Decode the cached AssetManifest. On parse failure (corrupt row),
    /// return an empty manifest and log a warning rather than failing the
    /// request — the page still renders, it just lacks block-specific assets.
    pub fn assets(&self) -> AssetManifest {
        match serde_json::from_str::<AssetManifest>(&self.assets_json) {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!(post.id = self.id, error = ?e, "corrupt assets_json, falling back to empty manifest");
                AssetManifest::default()
            }
        }
    }
}

/// Paginated list of published posts joined to a tag slug.
pub async fn list_by_tag(
    pool: &SqlitePool,
    tag_slug: &str,
    limit: i64,
    offset: i64,
) -> Result<Vec<Post>, sqlx::Error> {
    sqlx::query_as::<_, Post>(
        r#"
        SELECT p.id, p.slug, p.title, p.subtitle, p.status, p.author_id,
               p.published_at, p.scheduled_for, p.updated_at, p.created_at,
               p.excerpt, p.cover_image, p.reading_minutes,
               p.body_md, p.body_html, p.meta_json, p.assets_json
          FROM posts p
          JOIN post_tags pt ON pt.post_id = p.id
          JOIN tags t       ON t.id = pt.tag_id
         WHERE p.status = 'published'
           AND t.slug = ?
         ORDER BY p.published_at DESC
         LIMIT ? OFFSET ?
        "#,
    )
    .bind(tag_slug)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
}

pub async fn count_by_tag(pool: &SqlitePool, tag_slug: &str) -> Result<i64, sqlx::Error> {
    let (n,): (i64,) = sqlx::query_as(
        r#"
        SELECT COUNT(*) FROM posts p
          JOIN post_tags pt ON pt.post_id = p.id
          JOIN tags t       ON t.id = pt.tag_id
         WHERE p.status = 'published' AND t.slug = ?
        "#,
    )
    .bind(tag_slug)
    .fetch_one(pool)
    .await?;
    Ok(n)
}

/// Posts in a series, ordered by `series_order` (NULLS LAST), then `published_at`.
pub async fn list_by_series(
    pool: &SqlitePool,
    series_slug: &str,
) -> Result<Vec<Post>, sqlx::Error> {
    // Series is stored in meta_json as { "series": "...", "series_order": N }.
    sqlx::query_as::<_, Post>(
        r#"
        SELECT id, slug, title, subtitle, status, author_id,
               published_at, scheduled_for, updated_at, created_at,
               excerpt, cover_image, reading_minutes,
               body_md, body_html, meta_json, assets_json
          FROM posts
         WHERE status = 'published'
           AND json_extract(meta_json, '$.series') = ?
         ORDER BY COALESCE(json_extract(meta_json, '$.series_order'), 999999) ASC,
                  published_at DESC
        "#,
    )
    .bind(series_slug)
    .fetch_all(pool)
    .await
}

/// Distinct series slugs across all published posts.
pub async fn list_series_slugs(pool: &SqlitePool) -> Result<Vec<String>, sqlx::Error> {
    let rows: Vec<(String,)> = sqlx::query_as(
        r#"
        SELECT DISTINCT json_extract(meta_json, '$.series') AS s
          FROM posts
         WHERE status = 'published'
           AND json_extract(meta_json, '$.series') IS NOT NULL
        "#,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|(s,)| s).collect())
}

/// FTS5-backed full-text search. Returns rows annotated with a highlighted
/// snippet (HTML-safe markers `<mark>` / `</mark>`).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SearchHit {
    pub id: i64,
    pub slug: String,
    pub title: String,
    pub snippet: String,
    pub published_at: Option<i64>,
}

/// Escape a raw query so FTS5 receives a safe MATCH expression.
/// We wrap every whitespace-separated term in double quotes after escaping
/// embedded quotes. This neutralises FTS5 operators (`*`, `^`, `(`, `)`,
/// `AND`, `OR`, `NOT`, `NEAR`) by reducing them to phrase queries.
pub fn fts_escape(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len() + 8);
    let mut first = true;
    for term in raw.split_whitespace() {
        if !first {
            out.push(' ');
        }
        first = false;
        out.push('"');
        for ch in term.chars() {
            if ch == '"' {
                out.push('"');
                out.push('"'); // FTS5 quote escaping = double the quote
            } else {
                out.push(ch);
            }
        }
        out.push('"');
    }
    out
}

pub async fn search_fts(
    pool: &SqlitePool,
    query: &str,
    limit: i64,
    offset: i64,
) -> Result<Vec<SearchHit>, sqlx::Error> {
    let q = fts_escape(query);
    if q.is_empty() {
        return Ok(Vec::new());
    }
    sqlx::query_as::<_, SearchHit>(
        r#"
        SELECT p.id, p.slug, p.title,
               snippet(posts_fts, 2, '<mark>', '</mark>', '…', 24) AS snippet,
               p.published_at
          FROM posts p
          JOIN posts_fts ON posts_fts.rowid = p.id
         WHERE p.status = 'published'
           AND posts_fts MATCH ?
         ORDER BY rank
         LIMIT ? OFFSET ?
        "#,
    )
    .bind(&q)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
}

#[cfg(test)]
mod fts_escape_tests {
    use super::fts_escape;

    #[test]
    fn empty_input_yields_empty() {
        assert_eq!(fts_escape(""), "");
        assert_eq!(fts_escape("   "), "");
    }

    #[test]
    fn single_term_is_quoted() {
        assert_eq!(fts_escape("rust"), r#""rust""#);
    }

    #[test]
    fn star_is_neutralised() {
        assert_eq!(fts_escape("*"), r#""*""#);
    }

    #[test]
    fn operators_become_phrases() {
        // "AND" should be searched as the literal word, not an operator.
        assert_eq!(fts_escape("rust AND fast"), r#""rust" "AND" "fast""#);
    }

    #[test]
    fn embedded_quote_is_doubled() {
        assert_eq!(fts_escape(r#"say "hi""#), r#""say" """hi""""#);
    }
}

#[cfg(test)]
mod reader_query_tests {
    use super::*;
    use crate::test_support::fresh_pool;

    async fn seed_post(pool: &SqlitePool, slug: &str, title: &str, meta: &str) -> i64 {
        let now = 1_700_000_000_i64;
        sqlx::query("INSERT INTO users (id, email, password_hash, role, created_at) VALUES (1, 'a@b', 'x', 'admin', ?) ON CONFLICT(id) DO NOTHING")
            .bind(now)
            .execute(pool)
            .await
            .unwrap();

        sqlx::query(
            r#"
            INSERT INTO posts (slug, title, status, author_id, published_at,
                               updated_at, created_at, body_md, body_html, meta_json, assets_json)
            VALUES (?, ?, 'published', 1, ?, ?, ?, '# x', '<h1>x</h1>', ?, '[]')
            "#,
        )
        .bind(slug)
        .bind(title)
        .bind(now)
        .bind(now)
        .bind(now)
        .bind(meta)
        .execute(pool)
        .await
        .unwrap();

        let (id,): (i64,) = sqlx::query_as("SELECT id FROM posts WHERE slug = ?")
            .bind(slug)
            .fetch_one(pool)
            .await
            .unwrap();
        id
    }

    #[tokio::test]
    async fn series_grouping_orders_by_series_order() {
        let pool = fresh_pool().await;
        seed_post(&pool, "a", "A", r#"{"series":"s","series_order":2}"#).await;
        seed_post(&pool, "b", "B", r#"{"series":"s","series_order":1}"#).await;
        let posts = list_by_series(&pool, "s").await.unwrap();
        assert_eq!(posts.len(), 2);
        assert_eq!(posts[0].slug, "b");
        assert_eq!(posts[1].slug, "a");
    }

    #[tokio::test]
    async fn search_fts_returns_snippet_with_mark_tags() {
        let pool = fresh_pool().await;
        let id = seed_post(&pool, "x", "Rust on Cortex-M4", r#"{}"#).await;
        sqlx::query("UPDATE posts SET body_md = ?, excerpt = ? WHERE id = ?")
            .bind("Embedded Rust runs on Cortex hardware without a kernel.")
            .bind("Embedded Rust")
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();

        let hits = search_fts(&pool, "Cortex", 10, 0).await.unwrap();
        assert_eq!(hits.len(), 1);
        assert!(hits[0].snippet.contains("<mark>"));
    }

    #[tokio::test]
    async fn search_fts_special_chars_do_not_panic() {
        let pool = fresh_pool().await;
        seed_post(&pool, "x", "T", r#"{}"#).await;
        // None of these should produce a `SqliteError`; they should return 0 rows
        // or 1 if there happens to be a literal match.
        for q in [
            "*",
            "(",
            ")",
            "AND",
            "rust*",
            "rust OR fast",
            "\"unterminated",
        ] {
            let _ = search_fts(&pool, q, 10, 0).await.unwrap();
        }
    }
}

// ---------------------------------------------------------------------------
// Admin queries (Plan 1d)
// ---------------------------------------------------------------------------

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PostStatusFilter {
    All,
    Draft,
    Published,
    Scheduled,
}

impl PostStatusFilter {
    pub fn parse(s: &str) -> Self {
        match s {
            "draft" => Self::Draft,
            "published" => Self::Published,
            "scheduled" => Self::Scheduled,
            _ => Self::All,
        }
    }
}

impl fmt::Display for PostStatusFilter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::All => "all",
            Self::Draft => "draft",
            Self::Published => "published",
            Self::Scheduled => "scheduled",
        })
    }
}

#[derive(Debug, Clone)]
pub struct AdminPostRow {
    pub id: i64,
    pub slug: String,
    pub title: String,
    pub status: String,
    pub published_at: Option<i64>,
    pub updated_at: i64,
}

pub async fn list_admin(
    pool: &SqlitePool,
    status: PostStatusFilter,
    query: Option<&str>,
    limit: i64,
) -> Result<Vec<AdminPostRow>, DbError> {
    let q = query.map(|q| format!("%{}%", q.replace('%', r"\%")));
    let rows = match (status, q.as_deref()) {
        (PostStatusFilter::All, None) => {
            sqlx::query_as::<_, (i64, String, String, String, Option<i64>, i64)>(
                "SELECT id, slug, title, status, published_at, updated_at
                 FROM posts WHERE deleted_at IS NULL
                 ORDER BY updated_at DESC LIMIT ?",
            )
            .bind(limit)
            .fetch_all(pool)
            .await?
        }
        (PostStatusFilter::All, Some(qs)) => {
            sqlx::query_as::<_, (i64, String, String, String, Option<i64>, i64)>(
                "SELECT id, slug, title, status, published_at, updated_at
                 FROM posts WHERE deleted_at IS NULL AND (title LIKE ? OR slug LIKE ?)
                 ORDER BY updated_at DESC LIMIT ?",
            )
            .bind(qs)
            .bind(qs)
            .bind(limit)
            .fetch_all(pool)
            .await?
        }
        (s, None) => {
            sqlx::query_as::<_, (i64, String, String, String, Option<i64>, i64)>(
                "SELECT id, slug, title, status, published_at, updated_at
             FROM posts WHERE deleted_at IS NULL AND status = ?
             ORDER BY updated_at DESC LIMIT ?",
            )
            .bind(s.to_string())
            .bind(limit)
            .fetch_all(pool)
            .await?
        }
        (s, Some(qs)) => {
            sqlx::query_as::<_, (i64, String, String, String, Option<i64>, i64)>(
                "SELECT id, slug, title, status, published_at, updated_at
             FROM posts WHERE deleted_at IS NULL AND status = ? AND (title LIKE ? OR slug LIKE ?)
             ORDER BY updated_at DESC LIMIT ?",
            )
            .bind(s.to_string())
            .bind(qs)
            .bind(qs)
            .bind(limit)
            .fetch_all(pool)
            .await?
        }
    };

    Ok(rows
        .into_iter()
        .map(
            |(id, slug, title, status, published_at, updated_at)| AdminPostRow {
                id,
                slug,
                title,
                status,
                published_at,
                updated_at,
            },
        )
        .collect())
}

pub async fn dashboard_counts(pool: &SqlitePool) -> Result<(i64, i64, i64), DbError> {
    let drafts: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM posts WHERE status='draft' AND deleted_at IS NULL",
    )
    .fetch_one(pool)
    .await?;
    let scheduled: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM posts WHERE status='scheduled' AND deleted_at IS NULL",
    )
    .fetch_one(pool)
    .await?;
    let published: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM posts WHERE status='published' AND deleted_at IS NULL",
    )
    .fetch_one(pool)
    .await?;
    Ok((drafts, scheduled, published))
}

#[derive(Debug, Clone, Default)]
pub struct PostUpdate {
    pub title: Option<String>,
    pub slug: Option<String>,
    pub subtitle: Option<String>,
    pub excerpt: Option<String>,
    pub cover_image: Option<String>,
    pub body_md: Option<String>,
    pub body_html: Option<String>,
    pub status: Option<String>,
    pub scheduled_for: Option<Option<i64>>,
    pub tags_csv: Option<String>,
}

pub async fn update_fields(pool: &SqlitePool, id: i64, u: &PostUpdate) -> Result<(), DbError> {
    let now = time::OffsetDateTime::now_utc().unix_timestamp();
    let mut tx = pool.begin().await?;

    if let Some(v) = &u.title {
        sqlx::query("UPDATE posts SET title = ?, updated_at = ? WHERE id = ?")
            .bind(v)
            .bind(now)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    if let Some(v) = &u.slug {
        sqlx::query("UPDATE posts SET slug = ?, updated_at = ? WHERE id = ?")
            .bind(v)
            .bind(now)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    if let Some(v) = &u.subtitle {
        sqlx::query("UPDATE posts SET subtitle = ?, updated_at = ? WHERE id = ?")
            .bind(v)
            .bind(now)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    if let Some(v) = &u.excerpt {
        sqlx::query("UPDATE posts SET excerpt = ?, updated_at = ? WHERE id = ?")
            .bind(v)
            .bind(now)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    if let Some(v) = &u.cover_image {
        sqlx::query("UPDATE posts SET cover_image = ?, updated_at = ? WHERE id = ?")
            .bind(v)
            .bind(now)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    if let (Some(md), Some(html)) = (&u.body_md, &u.body_html) {
        sqlx::query(
            "UPDATE posts SET body_md = ?, body_html = ?, body_html_version = ?, updated_at = ? WHERE id = ?",
        )
            .bind(md)
            .bind(html)
            .bind(content::RENDER_VERSION as i64)
            .bind(now)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    if let Some(v) = &u.status {
        sqlx::query("UPDATE posts SET status = ?, updated_at = ? WHERE id = ?")
            .bind(v)
            .bind(now)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    if let Some(v) = &u.scheduled_for {
        sqlx::query("UPDATE posts SET scheduled_for = ?, updated_at = ? WHERE id = ?")
            .bind(v)
            .bind(now)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    if let Some(csv) = &u.tags_csv {
        // Replace tag set
        sqlx::query("DELETE FROM post_tags WHERE post_id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        for raw in csv.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            let slug = slugify(raw);
            sqlx::query("INSERT OR IGNORE INTO tags (slug, name) VALUES (?, ?)")
                .bind(&slug)
                .bind(raw)
                .execute(&mut *tx)
                .await?;
            let tag_id: i64 = sqlx::query_scalar("SELECT id FROM tags WHERE slug = ?")
                .bind(&slug)
                .fetch_one(&mut *tx)
                .await?;
            sqlx::query("INSERT OR IGNORE INTO post_tags (post_id, tag_id) VALUES (?, ?)")
                .bind(id)
                .bind(tag_id)
                .execute(&mut *tx)
                .await?;
        }
        sqlx::query("UPDATE posts SET updated_at = ? WHERE id = ?")
            .bind(now)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }

    tx.commit().await?;
    Ok(())
}

fn slugify(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_dash = false;
    for ch in s.chars().flat_map(|c| c.to_lowercase()) {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            prev_dash = false;
        } else if !prev_dash && !out.is_empty() {
            out.push('-');
            prev_dash = true;
        }
    }
    if out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() {
        out.push_str("tag");
    }
    out
}

/// Publish a post: flip status, set published_at, and enqueue one newsletter_outbox
/// row per confirmed, non-unsubscribed member. INSERT OR IGNORE guarantees
/// at-most-once even on duplicate clicks.
///
/// Returns the number of newly-enqueued outbox rows.
pub async fn publish(pool: &SqlitePool, id: i64) -> Result<u64, DbError> {
    let now = time::OffsetDateTime::now_utc().unix_timestamp();
    let mut tx = pool.begin().await?;

    sqlx::query(
        "UPDATE posts SET status = 'published',
                          published_at = COALESCE(published_at, ?),
                          updated_at = ?
         WHERE id = ? AND deleted_at IS NULL",
    )
    .bind(now)
    .bind(now)
    .bind(id)
    .execute(&mut *tx)
    .await?;

    let result = sqlx::query(
        "INSERT OR IGNORE INTO newsletter_outbox
            (post_id, member_id, status, attempts, created_at)
         SELECT ?, id, 'pending', 0, ?
         FROM members
         WHERE confirmed_at IS NOT NULL AND unsubscribed_at IS NULL",
    )
    .bind(id)
    .bind(now)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(result.rows_affected())
}

pub async fn soft_delete(pool: &SqlitePool, id: i64) -> Result<(), DbError> {
    let now = time::OffsetDateTime::now_utc().unix_timestamp();
    sqlx::query("UPDATE posts SET deleted_at = ?, updated_at = ? WHERE id = ?")
        .bind(now)
        .bind(now)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

#[cfg(test)]
mod admin_tests {
    use super::*;
    use crate::members;
    use crate::test_support::fresh_pool;
    use crate::users;

    async fn make_post(pool: &SqlitePool, title: &str, status: &str) -> i64 {
        let now = time::OffsetDateTime::now_utc().unix_timestamp();
        users::bootstrap_admin(pool, "admin@test", "hash")
            .await
            .ok();
        let user_id: i64 = sqlx::query_scalar("SELECT id FROM users LIMIT 1")
            .fetch_one(pool)
            .await
            .unwrap();
        let slug = slugify(title);
        sqlx::query(
            "INSERT INTO posts (slug, title, status, author_id, updated_at, created_at, body_md, body_html)
             VALUES (?, ?, ?, ?, ?, ?, '', '')",
        )
        .bind(&slug).bind(title).bind(status).bind(user_id).bind(now).bind(now)
        .execute(pool).await.unwrap();
        sqlx::query_scalar::<_, i64>("SELECT id FROM posts WHERE slug = ?")
            .bind(&slug)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn list_admin_filters_status() {
        let pool = fresh_pool().await;
        make_post(&pool, "A", "draft").await;
        make_post(&pool, "B", "published").await;
        make_post(&pool, "C", "draft").await;
        let drafts = list_admin(&pool, PostStatusFilter::Draft, None, 50)
            .await
            .unwrap();
        assert_eq!(drafts.len(), 2);
    }

    #[tokio::test]
    async fn list_admin_search_matches_title_or_slug() {
        let pool = fresh_pool().await;
        make_post(&pool, "Rust patterns", "draft").await;
        make_post(&pool, "Go patterns", "draft").await;
        let r = list_admin(&pool, PostStatusFilter::All, Some("rust"), 50)
            .await
            .unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].title, "Rust patterns");
    }

    #[tokio::test]
    async fn update_fields_round_trip() {
        let pool = fresh_pool().await;
        let id = make_post(&pool, "Original", "draft").await;
        update_fields(
            &pool,
            id,
            &PostUpdate {
                title: Some("Changed".into()),
                slug: Some("changed".into()),
                body_md: Some("# Hello".into()),
                body_html: Some("<h1>Hello</h1>".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let row: (String, String, String, String) =
            sqlx::query_as("SELECT title, slug, body_md, body_html FROM posts WHERE id = ?")
                .bind(id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(row.0, "Changed");
        assert_eq!(row.1, "changed");
        assert_eq!(row.2, "# Hello");
        assert_eq!(row.3, "<h1>Hello</h1>");
    }

    #[tokio::test]
    async fn publish_fans_out_outbox_to_confirmed_members() {
        let pool = fresh_pool().await;
        let id = make_post(&pool, "Newsletter", "draft").await;
        let now = time::OffsetDateTime::now_utc().unix_timestamp();
        // 3 confirmed, 1 unsubscribed, 1 unconfirmed
        for (email, confirmed, unsub) in [
            ("a@x.com", Some(now), None),
            ("b@x.com", Some(now), None),
            ("c@x.com", Some(now), None),
            ("d@x.com", Some(now), Some(now)),
            ("e@x.com", None, None),
        ] {
            members::insert_fixture(&pool, email, confirmed, unsub)
                .await
                .unwrap();
        }

        let enqueued = publish(&pool, id).await.unwrap();
        assert_eq!(enqueued, 3);

        let status: String = sqlx::query_scalar("SELECT status FROM posts WHERE id = ?")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(status, "published");

        let pending: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM newsletter_outbox WHERE post_id = ? AND status = 'pending'",
        )
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(pending, 3);
    }

    #[tokio::test]
    async fn publish_is_idempotent() {
        let pool = fresh_pool().await;
        let id = make_post(&pool, "Idem", "draft").await;
        let now = time::OffsetDateTime::now_utc().unix_timestamp();
        members::insert_fixture(&pool, "a@x.com", Some(now), None)
            .await
            .unwrap();
        let first = publish(&pool, id).await.unwrap();
        let second = publish(&pool, id).await.unwrap();
        assert_eq!(first, 1);
        assert_eq!(second, 0);
    }

    #[tokio::test]
    async fn soft_delete_hides_post_from_list_admin() {
        let pool = fresh_pool().await;
        let id = make_post(&pool, "Ghost", "draft").await;
        soft_delete(&pool, id).await.unwrap();
        let rows = list_admin(&pool, PostStatusFilter::All, None, 50)
            .await
            .unwrap();
        assert!(rows.iter().all(|r| r.id != id));
    }
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
