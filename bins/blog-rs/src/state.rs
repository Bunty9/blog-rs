use std::sync::Arc;

use db::SqlitePool;

use crate::config::Config;

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub config: Arc<Config>,
    // Consumed in Plan 1c+ for member confirm/unsubscribe HMAC tokens.
    #[allow(dead_code)]
    pub signing_key: Arc<Vec<u8>>,
}

impl AppState {
    pub fn new(pool: SqlitePool, config: Config, signing_key: Vec<u8>) -> Self {
        Self {
            pool,
            config: Arc::new(config),
            signing_key: Arc::new(signing_key),
        }
    }
}
