//! Post revision snapshots: one row per throttled save (see
//! [`should_snapshot`]) or publish, holding the author-editable content
//! fields a save can change (title, subtitle, body_md, meta_json). Restoring
//! writes these back through the normal post-update path in the HTTP layer.

use sqlx::SqlitePool;
use time::OffsetDateTime;

use crate::DbError;

/// How many revisions to keep per post; older rows are pruned on snapshot.
const MAX_REVISIONS_PER_POST: i64 = 50;

/// Minimum age (seconds) the latest revision must have before a save is
/// allowed to create another one. Keeps the autosave loop (every ~800ms of
/// typing) from spamming a row per keystroke pause.
pub const SNAPSHOT_MIN_INTERVAL_SECS: i64 = 10 * 60;

#[derive(Debug, Clone, sqlx::FromRow, PartialEq, Eq)]
pub struct PostRevision {
    pub id: i64,
    pub post_id: i64,
    pub title: String,
    pub subtitle: Option<String>,
    pub body_md: String,
    pub meta_json: Option<String>,
    pub created_at: i64,
}

/// Lightweight row for the sidebar list — no `body_md`, plus its byte length
/// so the UI can show a size without shipping the content.
#[derive(Debug, Clone, sqlx::FromRow, PartialEq, Eq)]
pub struct RevisionSummary {
    pub id: i64,
    pub title: String,
    pub created_at: i64,
    pub body_size: i64,
}

/// Insert a snapshot of the given content and prune anything past
/// [`MAX_REVISIONS_PER_POST`] for that post. Returns the new revision id.
pub async fn snapshot(
    pool: &SqlitePool,
    post_id: i64,
    title: &str,
    subtitle: Option<&str>,
    body_md: &str,
    meta_json: Option<&str>,
) -> Result<i64, DbError> {
    let now = OffsetDateTime::now_utc().unix_timestamp();
    let res = sqlx::query(
        "INSERT INTO post_revisions (post_id, title, subtitle, body_md, meta_json, created_at)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(post_id)
    .bind(title)
    .bind(subtitle)
    .bind(body_md)
    .bind(meta_json)
    .bind(now)
    .execute(pool)
    .await?;
    let id = res.last_insert_rowid();

    sqlx::query(
        "DELETE FROM post_revisions WHERE post_id = ? AND id NOT IN (
            SELECT id FROM post_revisions WHERE post_id = ?
             ORDER BY created_at DESC, id DESC LIMIT ?
        )",
    )
    .bind(post_id)
    .bind(post_id)
    .bind(MAX_REVISIONS_PER_POST)
    .execute(pool)
    .await?;

    Ok(id)
}

/// Newest-first summaries for a post, without `body_md`.
pub async fn list_for_post(
    pool: &SqlitePool,
    post_id: i64,
    limit: i64,
) -> Result<Vec<RevisionSummary>, DbError> {
    let rows = sqlx::query_as::<_, RevisionSummary>(
        "SELECT id, title, created_at, length(body_md) AS body_size
         FROM post_revisions WHERE post_id = ?
         ORDER BY created_at DESC, id DESC LIMIT ?",
    )
    .bind(post_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// The `created_at` of the most recent revision for a post, if any.
pub async fn latest_created_at(pool: &SqlitePool, post_id: i64) -> Result<Option<i64>, DbError> {
    let ts: Option<i64> = sqlx::query_scalar(
        "SELECT created_at FROM post_revisions WHERE post_id = ?
         ORDER BY created_at DESC, id DESC LIMIT 1",
    )
    .bind(post_id)
    .fetch_optional(pool)
    .await?
    .flatten();
    Ok(ts)
}

pub async fn get(pool: &SqlitePool, id: i64) -> Result<PostRevision, DbError> {
    sqlx::query_as::<_, PostRevision>("SELECT * FROM post_revisions WHERE id = ?")
        .bind(id)
        .fetch_one(pool)
        .await
        .map_err(DbError::from_row)
}

/// Whether a save should snapshot the pre-save content as a new revision:
/// only when the content actually changed, and only when the latest
/// existing revision (if any) is older than [`SNAPSHOT_MIN_INTERVAL_SECS`].
/// This is the throttle that keeps the ~800ms autosave loop from writing a
/// revision per keystroke pause.
pub fn should_snapshot(changed: bool, latest_revision_at: Option<i64>, now: i64) -> bool {
    if !changed {
        return false;
    }
    match latest_revision_at {
        None => true,
        Some(ts) => now - ts > SNAPSHOT_MIN_INTERVAL_SECS,
    }
}

#[cfg(test)]
mod policy_tests {
    use super::*;

    #[test]
    fn unchanged_content_never_snapshots() {
        assert!(!should_snapshot(false, None, 1_000));
        assert!(!should_snapshot(false, Some(0), 1_000_000));
    }

    #[test]
    fn changed_with_no_prior_revision_snapshots() {
        assert!(should_snapshot(true, None, 1_000));
    }

    #[test]
    fn changed_content_is_throttled_by_min_interval() {
        let latest = 1_000;
        assert!(!should_snapshot(
            true,
            Some(latest),
            latest + SNAPSHOT_MIN_INTERVAL_SECS - 1
        ));
        assert!(!should_snapshot(
            true,
            Some(latest),
            latest + SNAPSHOT_MIN_INTERVAL_SECS
        ));
        assert!(should_snapshot(
            true,
            Some(latest),
            latest + SNAPSHOT_MIN_INTERVAL_SECS + 1
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::posts::{self, NewPost};
    use crate::test_support::fresh_pool;
    use crate::users;

    async fn seed_post(pool: &SqlitePool) -> i64 {
        users::bootstrap_admin(pool, "a@b.c", "h").await.unwrap();
        let uid = users::find_by_email(pool, "a@b.c").await.unwrap().id;
        posts::create(
            pool,
            NewPost {
                slug: "hello",
                title: "Hello",
                subtitle: None,
                status: "draft",
                author_id: uid,
                excerpt: None,
                cover_image: None,
                body_md: "# hi",
                body_html: "<h1>hi</h1>",
                meta_json: None,
                toc_json: "[]",
                reading_minutes: Some(1),
            },
        )
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn snapshot_then_get_round_trips() {
        let pool = fresh_pool().await;
        let post_id = seed_post(&pool).await;
        let id = snapshot(&pool, post_id, "T", Some("Sub"), "# body", Some("{}"))
            .await
            .unwrap();
        let rev = get(&pool, id).await.unwrap();
        assert_eq!(rev.post_id, post_id);
        assert_eq!(rev.title, "T");
        assert_eq!(rev.subtitle.as_deref(), Some("Sub"));
        assert_eq!(rev.body_md, "# body");
    }

    #[tokio::test]
    async fn get_missing_is_not_found() {
        let pool = fresh_pool().await;
        let err = get(&pool, 999).await.unwrap_err();
        assert!(matches!(err, DbError::NotFound));
    }

    #[tokio::test]
    async fn list_for_post_is_newest_first_without_body() {
        let pool = fresh_pool().await;
        let post_id = seed_post(&pool).await;
        // `created_at` has 1-second resolution, so a tight loop like this can
        // tie; `id DESC` is the tiebreaker (see `list_for_post`'s ORDER BY),
        // which is enough to make insertion order deterministic here.
        for i in 0..3 {
            snapshot(&pool, post_id, &format!("T{i}"), None, "# body", None)
                .await
                .unwrap();
        }
        let rows = list_for_post(&pool, post_id, 50).await.unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].title, "T2");
        assert_eq!(rows[2].title, "T0");
        assert_eq!(rows[0].body_size, "# body".len() as i64);
    }

    #[tokio::test]
    async fn prune_keeps_newest_50_per_post() {
        let pool = fresh_pool().await;
        let post_id = seed_post(&pool).await;
        for i in 0..55 {
            snapshot(&pool, post_id, &format!("T{i}"), None, "# body", None)
                .await
                .unwrap();
        }
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM post_revisions WHERE post_id = ?")
                .bind(post_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(count, MAX_REVISIONS_PER_POST);

        let rows = list_for_post(&pool, post_id, 50).await.unwrap();
        // The 5 oldest (T0..T4) should have been pruned.
        assert!(rows.iter().all(|r| r.title != "T0"));
        assert_eq!(rows[0].title, "T54");
    }

    #[tokio::test]
    async fn hard_deleting_a_post_cascades_its_revisions() {
        // Posts are normally soft-deleted (see migrations/0006), but the FK
        // is still worth having: if a post row is ever actually removed
        // (data purge, manual cleanup), its revisions shouldn't be left
        // dangling.
        let pool = fresh_pool().await;
        let post_id = seed_post(&pool).await;
        snapshot(&pool, post_id, "T", None, "# body", None)
            .await
            .unwrap();

        sqlx::query("DELETE FROM posts WHERE id = ?")
            .bind(post_id)
            .execute(&pool)
            .await
            .unwrap();

        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM post_revisions WHERE post_id = ?")
                .bind(post_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(count, 0);
    }
}
