//! Privacy-friendly page view analytics: aggregate daily counters only.
//!
//! No cookies, no IP addresses, no user agents, and no per-visitor
//! identifiers are ever stored. `record_view` just increments a
//! `(day, path)` counter (and, optionally, a `(day, referrer host)`
//! counter). There is nothing here that can identify a visitor or link two
//! page views to the same person.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use time::{Date, Duration};

use crate::DbError;

/// Format a `Date` as `YYYY-MM-DD` without depending on the `time` crate's
/// `formatting` feature (not enabled in this workspace).
pub fn fmt_day(d: Date) -> String {
    format!("{:04}-{:02}-{:02}", d.year(), u8::from(d.month()), d.day())
}

/// Record one view of `path` on `day` (UTC, `YYYY-MM-DD`), optionally
/// attributing it to a referrer host. Upserts both aggregate tables.
pub async fn record_view(
    pool: &SqlitePool,
    day: &str,
    path: &str,
    referrer_host: Option<&str>,
) -> Result<(), DbError> {
    sqlx::query(
        "INSERT INTO page_views_daily (day, path, views) VALUES (?, ?, 1)
         ON CONFLICT(day, path) DO UPDATE SET views = views + 1",
    )
    .bind(day)
    .bind(path)
    .execute(pool)
    .await?;

    if let Some(host) = referrer_host {
        sqlx::query(
            "INSERT INTO referrers_daily (day, host, views) VALUES (?, ?, 1)
             ON CONFLICT(day, host) DO UPDATE SET views = views + 1",
        )
        .bind(day)
        .bind(host)
        .execute(pool)
        .await?;
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DailyTotal {
    pub day: String,
    pub views: i64,
}

/// Total views per day for the `days` days ending on (and including) `today`,
/// oldest first. Days with no rows are filled with 0 -- SQLite has no
/// `generate_series` in the version this workspace depends on, so the gaps
/// are filled here rather than in SQL.
pub async fn totals_by_day(
    pool: &SqlitePool,
    today: Date,
    days: i64,
) -> Result<Vec<DailyTotal>, DbError> {
    let start = today - Duration::days(days - 1);
    let start_s = fmt_day(start);
    let rows: Vec<(String, i64)> =
        sqlx::query_as("SELECT day, SUM(views) FROM page_views_daily WHERE day >= ? GROUP BY day")
            .bind(&start_s)
            .fetch_all(pool)
            .await?;
    let by_day: HashMap<String, i64> = rows.into_iter().collect();

    Ok((0..days)
        .map(|i| {
            let day = fmt_day(start + Duration::days(i));
            let views = by_day.get(&day).copied().unwrap_or(0);
            DailyTotal { day, views }
        })
        .collect())
}

/// Sum of views over the `days` days ending on (and including) `today`.
pub async fn total_views(pool: &SqlitePool, today: Date, days: i64) -> Result<i64, DbError> {
    let start = fmt_day(today - Duration::days(days - 1));
    let total: Option<i64> =
        sqlx::query_scalar("SELECT SUM(views) FROM page_views_daily WHERE day >= ?")
            .bind(start)
            .fetch_one(pool)
            .await?;
    Ok(total.unwrap_or(0))
}

/// Views recorded on exactly `day`.
pub async fn views_on(pool: &SqlitePool, day: Date) -> Result<i64, DbError> {
    let day_s = fmt_day(day);
    let total: Option<i64> =
        sqlx::query_scalar("SELECT SUM(views) FROM page_views_daily WHERE day = ?")
            .bind(day_s)
            .fetch_one(pool)
            .await?;
    Ok(total.unwrap_or(0))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PathViews {
    pub path: String,
    pub views: i64,
}

/// Top `limit` paths by total views over the `days` days ending on `today`.
pub async fn top_paths(
    pool: &SqlitePool,
    today: Date,
    days: i64,
    limit: i64,
) -> Result<Vec<PathViews>, DbError> {
    let start = fmt_day(today - Duration::days(days - 1));
    let rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT path, SUM(views) as v FROM page_views_daily
         WHERE day >= ? GROUP BY path ORDER BY v DESC, path ASC LIMIT ?",
    )
    .bind(start)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(path, views)| PathViews { path, views })
        .collect())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HostViews {
    pub host: String,
    pub views: i64,
}

/// Top `limit` referrer hosts by total views over the `days` days ending on
/// `today`.
pub async fn top_referrers(
    pool: &SqlitePool,
    today: Date,
    days: i64,
    limit: i64,
) -> Result<Vec<HostViews>, DbError> {
    let start = fmt_day(today - Duration::days(days - 1));
    let rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT host, SUM(views) as v FROM referrers_daily
         WHERE day >= ? GROUP BY host ORDER BY v DESC, host ASC LIMIT ?",
    )
    .bind(start)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(host, views)| HostViews { host, views })
        .collect())
}

/// Delete aggregate rows older than `cutoff_day` (`YYYY-MM-DD`, exclusive of
/// bound days on/after it). Returns the number of rows removed across both
/// tables. Called opportunistically (at most once a day) from the
/// background worker -- this is aggregate housekeeping, not a hot path.
pub async fn prune_older_than(pool: &SqlitePool, cutoff_day: &str) -> Result<u64, DbError> {
    let a = sqlx::query("DELETE FROM page_views_daily WHERE day < ?")
        .bind(cutoff_day)
        .execute(pool)
        .await?;
    let b = sqlx::query("DELETE FROM referrers_daily WHERE day < ?")
        .bind(cutoff_day)
        .execute(pool)
        .await?;
    Ok(a.rows_affected() + b.rows_affected())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::fresh_pool;
    use time::macros::date;

    #[tokio::test]
    async fn record_view_upserts_and_increments() {
        let pool = fresh_pool().await;
        record_view(&pool, "2026-09-25", "/posts/hello", None)
            .await
            .unwrap();
        record_view(&pool, "2026-09-25", "/posts/hello", None)
            .await
            .unwrap();

        let views: i64 =
            sqlx::query_scalar("SELECT views FROM page_views_daily WHERE day = ? AND path = ?")
                .bind("2026-09-25")
                .bind("/posts/hello")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(views, 2);
    }

    #[tokio::test]
    async fn record_view_with_referrer_upserts_referrer_table() {
        let pool = fresh_pool().await;
        record_view(
            &pool,
            "2026-09-25",
            "/posts/hello",
            Some("news.ycombinator.com"),
        )
        .await
        .unwrap();
        record_view(
            &pool,
            "2026-09-25",
            "/posts/hello",
            Some("news.ycombinator.com"),
        )
        .await
        .unwrap();

        let views: i64 =
            sqlx::query_scalar("SELECT views FROM referrers_daily WHERE day = ? AND host = ?")
                .bind("2026-09-25")
                .bind("news.ycombinator.com")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(views, 2);
    }

    #[tokio::test]
    async fn totals_by_day_fills_missing_days_with_zero() {
        let pool = fresh_pool().await;
        let today = date!(2026 - 09 - 25);
        record_view(&pool, "2026-09-25", "/a", None).await.unwrap();
        record_view(&pool, "2026-09-23", "/a", None).await.unwrap();

        let totals = totals_by_day(&pool, today, 5).await.unwrap();
        assert_eq!(totals.len(), 5);
        assert_eq!(totals[0].day, "2026-09-21");
        assert_eq!(totals[0].views, 0);
        assert_eq!(totals[2].day, "2026-09-23");
        assert_eq!(totals[2].views, 1);
        assert_eq!(totals[4].day, "2026-09-25");
        assert_eq!(totals[4].views, 1);
    }

    #[tokio::test]
    async fn top_paths_orders_by_views_desc() {
        let pool = fresh_pool().await;
        let today = date!(2026 - 09 - 25);
        for _ in 0..3 {
            record_view(&pool, "2026-09-25", "/popular", None)
                .await
                .unwrap();
        }
        record_view(&pool, "2026-09-25", "/quiet", None)
            .await
            .unwrap();

        let top = top_paths(&pool, today, 7, 10).await.unwrap();
        assert_eq!(top[0].path, "/popular");
        assert_eq!(top[0].views, 3);
        assert_eq!(top[1].path, "/quiet");
        assert_eq!(top[1].views, 1);
    }

    #[tokio::test]
    async fn top_referrers_orders_by_views_desc() {
        let pool = fresh_pool().await;
        let today = date!(2026 - 09 - 25);
        record_view(&pool, "2026-09-25", "/a", Some("lobste.rs"))
            .await
            .unwrap();
        record_view(&pool, "2026-09-25", "/b", Some("lobste.rs"))
            .await
            .unwrap();
        record_view(&pool, "2026-09-25", "/c", Some("reddit.com"))
            .await
            .unwrap();

        let top = top_referrers(&pool, today, 7, 10).await.unwrap();
        assert_eq!(top[0].host, "lobste.rs");
        assert_eq!(top[0].views, 2);
        assert_eq!(top[1].host, "reddit.com");
        assert_eq!(top[1].views, 1);
    }

    #[tokio::test]
    async fn total_views_sums_range_only() {
        let pool = fresh_pool().await;
        let today = date!(2026 - 09 - 25);
        record_view(&pool, "2026-09-25", "/a", None).await.unwrap();
        record_view(&pool, "2026-01-01", "/a", None).await.unwrap(); // outside range

        let total = total_views(&pool, today, 7).await.unwrap();
        assert_eq!(total, 1);
    }

    #[tokio::test]
    async fn prune_older_than_deletes_old_rows_only() {
        let pool = fresh_pool().await;
        record_view(&pool, "2020-01-01", "/old", Some("old.example"))
            .await
            .unwrap();
        record_view(&pool, "2026-09-25", "/new", Some("new.example"))
            .await
            .unwrap();

        let deleted = prune_older_than(&pool, "2025-01-01").await.unwrap();
        assert_eq!(deleted, 2); // one row in each table

        let remaining: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM page_views_daily")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(remaining, 1);
        let remaining_ref: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM referrers_daily")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(remaining_ref, 1);
    }
}
