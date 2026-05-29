//! Series-level queries. A series is not its own table — it's a string in
//! `posts.meta_json.series`. These helpers materialise the implicit grouping.

use sqlx::SqlitePool;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SeriesMeta {
    pub slug: String,
    pub post_count: i64,
}

/// Resolve a series slug to a small descriptor. Returns `None` if the slug
/// is not present on any published post.
pub async fn get_meta(pool: &SqlitePool, slug: &str) -> Result<Option<SeriesMeta>, sqlx::Error> {
    let row: Option<(i64,)> = sqlx::query_as(
        r#"
        SELECT COUNT(*) FROM posts
         WHERE status = 'published'
           AND json_extract(meta_json, '$.series') = ?
        "#,
    )
    .bind(slug)
    .fetch_optional(pool)
    .await?;
    match row {
        Some((n,)) if n > 0 => Ok(Some(SeriesMeta {
            slug: slug.into(),
            post_count: n,
        })),
        _ => Ok(None),
    }
}
