use sqlx::SqlitePool;

use crate::DbError;

/// Embeds the contents of `blog-rs/migrations/` at compile time and runs them
/// in order against the given pool.
pub async fn run(pool: &SqlitePool) -> Result<(), DbError> {
    sqlx::migrate!("../../migrations").run(pool).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn migrations_apply_cleanly() {
        let pool = crate::pool::memory_pool().await.unwrap();
        run(&pool).await.unwrap();

        // Verify a representative table exists.
        let count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='users'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(count.0, 1);

        // FTS5 virtual table exists.
        let fts: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM sqlite_master WHERE name='posts_fts'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(fts.0, 1);
    }
}
