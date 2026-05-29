//! Key/value site settings backed by the `settings` table.

use crate::DbError;
use sqlx::{Row, SqlitePool};
use std::collections::BTreeMap;

pub const ALL_KEYS: &[&str] = &[
    "site_title",
    "site_subtitle",
    "site_url",
    "default_author_email",
    "smtp_host",
    "smtp_port",
    "smtp_user",
    "smtp_password",
    "smtp_from",
];

/// Fetch a single setting value. Returns Ok(None) if the key is unknown or absent.
pub async fn get(pool: &SqlitePool, key: &str) -> Result<Option<String>, DbError> {
    let row = sqlx::query("SELECT value FROM settings WHERE key = ?")
        .bind(key)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|r| r.get::<String, _>("value")))
}

/// Fetch every known setting as a map. Missing keys are filled with empty strings
/// so the UI never crashes on a fresh DB.
pub async fn get_all(pool: &SqlitePool) -> Result<BTreeMap<String, String>, DbError> {
    let rows = sqlx::query("SELECT key, value FROM settings")
        .fetch_all(pool)
        .await?;
    let mut map: BTreeMap<String, String> = ALL_KEYS
        .iter()
        .map(|k| ((*k).to_string(), String::new()))
        .collect();
    for r in rows {
        let k: String = r.get("key");
        let v: String = r.get("value");
        map.insert(k, v);
    }
    Ok(map)
}

/// Upsert a single setting.
pub async fn set(pool: &SqlitePool, key: &str, value: &str) -> Result<(), DbError> {
    if !ALL_KEYS.contains(&key) {
        return Err(DbError::Invalid(format!("unknown setting key `{key}`")));
    }
    let now = time::OffsetDateTime::now_utc().unix_timestamp();
    sqlx::query(
        "INSERT INTO settings (key, value, updated_at) VALUES (?, ?, ?)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
    )
    .bind(key)
    .bind(value)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(())
}

/// Bulk-set many keys atomically.
pub async fn set_many(pool: &SqlitePool, pairs: &[(String, String)]) -> Result<(), DbError> {
    let now = time::OffsetDateTime::now_utc().unix_timestamp();
    let mut tx = pool.begin().await?;
    for (k, v) in pairs {
        if !ALL_KEYS.contains(&k.as_str()) {
            return Err(DbError::Invalid(format!("unknown setting key `{k}`")));
        }
        sqlx::query(
            "INSERT INTO settings (key, value, updated_at) VALUES (?, ?, ?)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        )
        .bind(k)
        .bind(v)
        .bind(now)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::fresh_pool;

    #[tokio::test]
    async fn get_returns_none_for_unknown() {
        let pool = fresh_pool().await;
        assert_eq!(get(&pool, "nope").await.unwrap(), None);
    }

    #[tokio::test]
    async fn seeded_keys_are_present() {
        let pool = fresh_pool().await;
        let all = get_all(&pool).await.unwrap();
        for k in ALL_KEYS {
            assert!(all.contains_key(*k), "missing key {k}");
        }
    }

    #[tokio::test]
    async fn set_then_get_round_trip() {
        let pool = fresh_pool().await;
        set(&pool, "site_title", "My Blog").await.unwrap();
        assert_eq!(
            get(&pool, "site_title").await.unwrap().as_deref(),
            Some("My Blog")
        );
    }

    #[tokio::test]
    async fn set_unknown_key_errors() {
        let pool = fresh_pool().await;
        let err = set(&pool, "bogus", "x").await.unwrap_err();
        assert!(matches!(err, DbError::Invalid(_)));
    }

    #[tokio::test]
    async fn set_many_atomic() {
        let pool = fresh_pool().await;
        set_many(
            &pool,
            &[
                ("site_title".to_string(), "A".to_string()),
                ("site_subtitle".to_string(), "B".to_string()),
            ],
        )
        .await
        .unwrap();
        assert_eq!(
            get(&pool, "site_title").await.unwrap().as_deref(),
            Some("A")
        );
        assert_eq!(
            get(&pool, "site_subtitle").await.unwrap().as_deref(),
            Some("B")
        );
    }
}
