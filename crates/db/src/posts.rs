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
