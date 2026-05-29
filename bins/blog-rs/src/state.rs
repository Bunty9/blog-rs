use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use db::SqlitePool;
use lettre::Message;
use tokio::sync::Mutex;

use crate::config::Config;
use crate::mailer::{MailError, MailerHandle, Transport};
use crate::tokens::TokenSigner;

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub config: Arc<Config>,
    // Kept for future re-derivation of subsystem keys; the token signer
    // already owns its working copy, so this field is read by no current
    // code path. Drop or surface via accessor when a second consumer lands.
    #[allow(dead_code)]
    pub signing_key: Arc<Vec<u8>>,
    pub tokens: TokenSigner,
    pub mailer: MailerHandle,
    pub site: SiteConfig,
    /// Last time the outbox worker completed a tick. `None` until the first
    /// tick lands. `/readyz` treats a stale value as a not-ready signal.
    pub worker_heartbeat: Arc<Mutex<Option<Instant>>>,
    /// When `AppState` was constructed. `/readyz` uses this to grant the
    /// worker a small warm-up grace window before failing on a `None`
    /// heartbeat.
    pub started_at: Instant,
}

#[derive(Clone, Debug)]
pub struct SiteConfig {
    pub base_url: String,
    pub site_title: String,
    pub admin_from: String,
}

impl SiteConfig {
    #[allow(dead_code)] // Called from main; tests use SiteConfig::default().
    pub fn from_env() -> Self {
        Self {
            base_url: std::env::var("BLOG_BASE_URL")
                .unwrap_or_else(|_| "http://localhost:8080".into()),
            site_title: std::env::var("BLOG_TITLE").unwrap_or_else(|_| "blog-rs".into()),
            admin_from: std::env::var("BLOG_FROM")
                .unwrap_or_else(|_| "blog-rs <noreply@localhost>".into()),
        }
    }
}

impl Default for SiteConfig {
    fn default() -> Self {
        Self {
            base_url: "http://localhost:8080".into(),
            site_title: "blog-rs".into(),
            admin_from: "blog-rs <noreply@localhost>".into(),
        }
    }
}

/// Mailer that discards every message. Used as the default in `AppState::new`
/// so unit tests that don't care about mail flow don't have to wire one up.
/// `main.rs` overrides via `with_mailer` after constructing the state.
pub struct NoopMailer;

#[async_trait]
impl Transport for NoopMailer {
    async fn send(&self, _msg: Message) -> Result<(), MailError> {
        Ok(())
    }
}

impl AppState {
    pub fn new(pool: SqlitePool, config: Config, signing_key: Vec<u8>) -> Self {
        let ttl = if config.confirm_token_ttl_seconds > 0 {
            config.confirm_token_ttl_seconds as u32
        } else {
            60 * 60 * 24
        };
        // Token signer derives its HMAC key from the signing_key bytes.
        // If signing_key is empty (test default), use a fixed-but-distinct
        // placeholder so tokens still round-trip in the same process.
        let secret = if signing_key.is_empty() {
            b"blog-rs-default-test-secret-not-for-prod".to_vec()
        } else {
            signing_key.clone()
        };
        let tokens = TokenSigner::new(secret, ttl);
        let mailer: MailerHandle = Arc::new(NoopMailer);
        Self {
            pool,
            config: Arc::new(config),
            signing_key: Arc::new(signing_key),
            tokens,
            mailer,
            site: SiteConfig::default(),
            worker_heartbeat: Arc::new(Mutex::new(None)),
            started_at: Instant::now(),
        }
    }

    /// Builder-style override used by the binary entry point to swap in the
    /// real env-derived mailer + site config without breaking existing test
    /// call sites of `AppState::new`.
    #[allow(dead_code)] // Used by main; tests construct AppState directly.
    pub fn with_mailer(mut self, mailer: MailerHandle) -> Self {
        self.mailer = mailer;
        self
    }

    #[allow(dead_code)] // Used by main; tests construct AppState directly.
    pub fn with_site(mut self, site: SiteConfig) -> Self {
        self.site = site;
        self
    }
}
