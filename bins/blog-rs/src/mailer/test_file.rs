//! File-backed transport used in tests. Appends each message as
//! a serialized RFC-5322 blob to a single file, separated by a boundary.

use super::{MailError, Transport};
use async_trait::async_trait;
use lettre::Message;
use std::path::PathBuf;
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;

const BOUNDARY: &[u8] = b"\r\n--BLOGRS-MAIL-SEPARATOR--\r\n";

pub struct FileTransport {
    path: PathBuf,
    lock: Mutex<()>,
}

impl FileTransport {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            lock: Mutex::new(()),
        }
    }
}

#[async_trait]
impl Transport for FileTransport {
    async fn send(&self, msg: Message) -> Result<(), MailError> {
        let raw = msg.formatted();
        let _g = self.lock.lock().await;
        let mut f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .await?;
        f.write_all(&raw).await?;
        f.write_all(BOUNDARY).await?;
        f.flush().await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lettre::message::header::ContentType;

    #[tokio::test]
    async fn appends_messages() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let path = tmp.path().to_owned();
        let t = FileTransport::new(&path);

        let msg = Message::builder()
            .from("Test <test@example.com>".parse().unwrap())
            .to("a@example.com".parse().unwrap())
            .subject("hi")
            .header(ContentType::TEXT_PLAIN)
            .body(String::from("body"))
            .unwrap();
        t.send(msg.clone()).await.unwrap();
        t.send(msg).await.unwrap();

        let contents = tokio::fs::read(&path).await.unwrap();
        let s = String::from_utf8_lossy(&contents);
        assert_eq!(s.matches("Subject: hi").count(), 2);
        assert_eq!(s.matches("BLOGRS-MAIL-SEPARATOR").count(), 2);
    }
}
