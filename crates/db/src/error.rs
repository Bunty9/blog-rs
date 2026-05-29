use thiserror::Error;

#[derive(Debug, Error)]
pub enum DbError {
    #[error("sqlx: {0}")]
    Sqlx(#[from] sqlx::Error),

    #[error("migrate: {0}")]
    Migrate(#[from] sqlx::migrate::MigrateError),

    #[error("not found")]
    NotFound,

    #[error("conflict: {0}")]
    Conflict(String),

    #[error("invalid: {0}")]
    Invalid(String),
}

impl DbError {
    /// Map a sqlx::Error to NotFound when it is RowNotFound, otherwise passthrough.
    pub fn from_row(e: sqlx::Error) -> Self {
        match e {
            sqlx::Error::RowNotFound => Self::NotFound,
            other => Self::Sqlx(other),
        }
    }
}
