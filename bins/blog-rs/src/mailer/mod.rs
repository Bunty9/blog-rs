//! Pluggable mail transport. Real SMTP for production; an append-to-file
//! transport for tests so the E2E harness can assert outgoing messages
//! without bringing up an actual SMTP server.

pub mod smtp;
pub mod test_file;

use async_trait::async_trait;
use lettre::Message;
use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Error)]
#[allow(dead_code)] // Smtp + Build are produced by the runtime impls only.
pub enum MailError {
    #[error("smtp transport: {0}")]
    Smtp(String),
    #[error("file transport: {0}")]
    File(#[from] std::io::Error),
    #[error("message build: {0}")]
    Build(String),
}

#[async_trait]
pub trait Transport: Send + Sync {
    async fn send(&self, msg: Message) -> Result<(), MailError>;
}

pub type MailerHandle = Arc<dyn Transport>;

/// Build a mailer based on env: `BLOG_RS_MAIL=test` → file transport,
/// anything else → SMTP from `SMTP_HOST` / `SMTP_PORT` / `SMTP_USER` / `SMTP_PASSWORD`.
#[allow(dead_code)] // Called from main; not reached by integration tests that
                    // pull this module via `#[path]`.
pub fn from_env() -> Result<MailerHandle, MailError> {
    match std::env::var("BLOG_RS_MAIL").as_deref() {
        Ok("test") => {
            let path =
                std::env::var("BLOG_RS_MAIL_FILE").unwrap_or_else(|_| "./test-mailbox.eml".into());
            Ok(Arc::new(test_file::FileTransport::new(path)))
        }
        _ => Ok(Arc::new(smtp::SmtpTransport::from_env()?)),
    }
}
