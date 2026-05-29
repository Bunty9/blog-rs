//! Real SMTP transport over rustls.

use super::{MailError, Transport};
use async_trait::async_trait;
use lettre::transport::smtp::authentication::Credentials;
use lettre::transport::smtp::AsyncSmtpTransport;
use lettre::{AsyncTransport, Message, Tokio1Executor};

pub struct SmtpTransport {
    inner: AsyncSmtpTransport<Tokio1Executor>,
}

impl SmtpTransport {
    pub fn from_env() -> Result<Self, MailError> {
        let host =
            std::env::var("SMTP_HOST").map_err(|_| MailError::Smtp("SMTP_HOST unset".into()))?;
        let port: u16 = std::env::var("SMTP_PORT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(587);
        let user =
            std::env::var("SMTP_USER").map_err(|_| MailError::Smtp("SMTP_USER unset".into()))?;
        let pass = std::env::var("SMTP_PASSWORD")
            .map_err(|_| MailError::Smtp("SMTP_PASSWORD unset".into()))?;
        let creds = Credentials::new(user, pass);
        let inner = AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&host)
            .map_err(|e| MailError::Smtp(e.to_string()))?
            .port(port)
            .credentials(creds)
            .build();
        Ok(Self { inner })
    }
}

#[async_trait]
impl Transport for SmtpTransport {
    async fn send(&self, msg: Message) -> Result<(), MailError> {
        self.inner
            .send(msg)
            .await
            .map(|_| ())
            .map_err(|e| MailError::Smtp(e.to_string()))
    }
}
