//! Persistence layer for blog-rs. Owns the SQLx pool, migrations, and one
//! module per table with typed queries. No HTTP, no rendering.

pub mod error;
pub mod migrations;
pub mod pool;

// Tables - filled in by later tasks.
pub mod members;
pub mod outbox;
pub mod pages;
pub mod posts;
pub mod search;
pub mod series;
pub mod sessions;
pub mod settings;
pub mod tags;
pub mod users;

pub use settings as settings_db;

pub use error::DbError;
pub use pages::Page;
pub use pool::connect;
pub use series::SeriesMeta;
pub use sqlx::SqlitePool;

/// Bring up a fresh pool and run migrations. The public entry point used by
/// the server binary and by test helpers.
pub async fn initialize(url: &str, max_connections: u32) -> Result<SqlitePool, DbError> {
    let pool = pool::connect(url, max_connections).await?;
    migrations::run(&pool).await?;
    Ok(pool)
}

#[cfg(any(test, feature = "test-helpers"))]
pub mod test_support {
    use super::*;

    /// Build an in-memory pool with all migrations applied. Tests use this.
    pub async fn fresh_pool() -> SqlitePool {
        let pool = pool::memory_pool().await.expect("memory pool");
        migrations::run(&pool).await.expect("migrations");
        pool
    }
}
