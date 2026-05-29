use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
    SqlitePool,
};
use std::str::FromStr;
use std::time::Duration;

use crate::DbError;

/// Build a SQLite pool with sane defaults: WAL, foreign keys on, sensible
/// timeouts. `url` accepts the SQLx form (`sqlite::memory:`, `sqlite://path/to.db`).
pub async fn connect(url: &str, max_connections: u32) -> Result<SqlitePool, DbError> {
    let opts = SqliteConnectOptions::from_str(url)?
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(5));

    let pool = SqlitePoolOptions::new()
        .max_connections(max_connections.max(1))
        .acquire_timeout(Duration::from_secs(10))
        .connect_with(opts)
        .await?;
    Ok(pool)
}

/// In-memory pool for tests. Single connection so the database survives across
/// query boundaries.
#[cfg(any(test, feature = "test-helpers"))]
pub async fn memory_pool() -> Result<SqlitePool, DbError> {
    SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::from_str("sqlite::memory:")?
                .foreign_keys(true)
                .journal_mode(SqliteJournalMode::Memory),
        )
        .await
        .map_err(DbError::from)
}
