//! HMAC-signed confirm + unsubscribe tokens.
//!
//! Layout: base64url_no_pad(payload || hmac_sha256(secret, payload)[..16])
//! payload = u32 LE member_id, u8 purpose, u32 LE issued_at, u32 LE nonce.
//!
//! This is intentionally separate from `auth::tokens` (which uses a JSON
//! claims payload). Member-facing links are short-lived and high-volume,
//! so we use a compact fixed-size encoding here.

use base64ct::{Base64UrlUnpadded, Encoding};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

type HmacSha256 = Hmac<Sha256>;

const PAYLOAD_LEN: usize = 13;
const MAC_LEN: usize = 16;
const TOKEN_LEN: usize = PAYLOAD_LEN + MAC_LEN;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Purpose {
    Confirm = 0,
    Unsubscribe = 1,
}

impl Purpose {
    fn from_u8(b: u8) -> Option<Self> {
        match b {
            0 => Some(Purpose::Confirm),
            1 => Some(Purpose::Unsubscribe),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenPayload {
    pub member_id: u32,
    pub purpose: Purpose,
    pub issued_at: u32,
    pub nonce: u32,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum TokenError {
    #[error("token decoding failed")]
    Decode,
    #[error("token length mismatch")]
    Length,
    #[error("token signature invalid")]
    Mac,
    #[error("token purpose mismatch")]
    Purpose,
    #[error("token expired")]
    Expired,
    #[error("system clock before UNIX epoch")]
    Clock,
    #[error("rng failure")]
    Rng,
}

#[derive(Clone)]
pub struct TokenSigner {
    secret: Vec<u8>,
    ttl_seconds: u32,
}

impl TokenSigner {
    pub fn new(secret: impl Into<Vec<u8>>, ttl_seconds: u32) -> Self {
        Self {
            secret: secret.into(),
            ttl_seconds,
        }
    }

    pub fn ttl(&self) -> u32 {
        self.ttl_seconds
    }

    pub fn issue(&self, member_id: u32, purpose: Purpose) -> Result<String, TokenError> {
        let issued_at = now_epoch()?;
        let mut nonce_bytes = [0u8; 4];
        getrandom::getrandom(&mut nonce_bytes).map_err(|_| TokenError::Rng)?;
        let nonce = u32::from_le_bytes(nonce_bytes);
        let payload = TokenPayload {
            member_id,
            purpose,
            issued_at,
            nonce,
        };
        Ok(self.encode(&payload))
    }

    fn encode(&self, p: &TokenPayload) -> String {
        let mut buf = [0u8; TOKEN_LEN];
        buf[0..4].copy_from_slice(&p.member_id.to_le_bytes());
        buf[4] = p.purpose as u8;
        buf[5..9].copy_from_slice(&p.issued_at.to_le_bytes());
        buf[9..13].copy_from_slice(&p.nonce.to_le_bytes());

        let mut mac = HmacSha256::new_from_slice(&self.secret).expect("hmac key");
        mac.update(&buf[..PAYLOAD_LEN]);
        let full_mac = mac.finalize().into_bytes();
        buf[PAYLOAD_LEN..].copy_from_slice(&full_mac[..MAC_LEN]);

        Base64UrlUnpadded::encode_string(&buf)
    }

    pub fn verify(
        &self,
        token: &str,
        expected_purpose: Purpose,
    ) -> Result<TokenPayload, TokenError> {
        let mut buf = [0u8; TOKEN_LEN];
        let decoded = Base64UrlUnpadded::decode(token.as_bytes(), &mut buf)
            .map_err(|_| TokenError::Decode)?;
        if decoded.len() != TOKEN_LEN {
            return Err(TokenError::Length);
        }
        let mut mac = HmacSha256::new_from_slice(&self.secret).expect("hmac key");
        mac.update(&buf[..PAYLOAD_LEN]);
        mac.verify_truncated_left(&buf[PAYLOAD_LEN..])
            .map_err(|_| TokenError::Mac)?;

        let member_id = u32::from_le_bytes(buf[0..4].try_into().unwrap());
        let purpose = Purpose::from_u8(buf[4]).ok_or(TokenError::Purpose)?;
        if purpose != expected_purpose {
            return Err(TokenError::Purpose);
        }
        let issued_at = u32::from_le_bytes(buf[5..9].try_into().unwrap());
        let nonce = u32::from_le_bytes(buf[9..13].try_into().unwrap());

        let now = now_epoch()?;
        if now.saturating_sub(issued_at) > self.ttl_seconds {
            return Err(TokenError::Expired);
        }

        Ok(TokenPayload {
            member_id,
            purpose,
            issued_at,
            nonce,
        })
    }
}

fn now_epoch() -> Result<u32, TokenError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| TokenError::Clock)
        .map(|d| d.as_secs() as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signer() -> TokenSigner {
        TokenSigner::new(b"unit-test-secret".to_vec(), 3600)
    }

    #[test]
    fn round_trip_confirm() {
        let s = signer();
        let tok = s.issue(42, Purpose::Confirm).unwrap();
        let p = s.verify(&tok, Purpose::Confirm).unwrap();
        assert_eq!(p.member_id, 42);
        assert_eq!(p.purpose, Purpose::Confirm);
    }

    #[test]
    fn round_trip_unsubscribe() {
        let s = signer();
        let tok = s.issue(7, Purpose::Unsubscribe).unwrap();
        let p = s.verify(&tok, Purpose::Unsubscribe).unwrap();
        assert_eq!(p.member_id, 7);
    }

    #[test]
    fn rejects_wrong_purpose() {
        let s = signer();
        let tok = s.issue(42, Purpose::Confirm).unwrap();
        assert_eq!(
            s.verify(&tok, Purpose::Unsubscribe),
            Err(TokenError::Purpose)
        );
    }

    #[test]
    fn rejects_tampered_payload() {
        let s = signer();
        let tok = s.issue(42, Purpose::Confirm).unwrap();
        let mut bytes = tok.into_bytes();
        // flip a byte in the payload (first byte of base64 always decodes into payload)
        bytes[0] = if bytes[0] == b'A' { b'B' } else { b'A' };
        let tampered = String::from_utf8(bytes).unwrap();
        let err = s.verify(&tampered, Purpose::Confirm).unwrap_err();
        assert!(matches!(
            err,
            TokenError::Mac | TokenError::Decode | TokenError::Length | TokenError::Purpose
        ));
    }

    #[test]
    fn rejects_wrong_secret() {
        let a = TokenSigner::new(b"alpha".to_vec(), 3600);
        let b = TokenSigner::new(b"beta".to_vec(), 3600);
        let tok = a.issue(1, Purpose::Confirm).unwrap();
        assert_eq!(b.verify(&tok, Purpose::Confirm), Err(TokenError::Mac));
    }

    #[test]
    fn rejects_expired() {
        let s = TokenSigner::new(b"k".to_vec(), 0); // TTL=0 → instantly expired
        let tok = s.issue(1, Purpose::Confirm).unwrap();
        std::thread::sleep(std::time::Duration::from_secs(1));
        assert_eq!(s.verify(&tok, Purpose::Confirm), Err(TokenError::Expired));
    }

    #[test]
    fn rejects_garbage_token() {
        let s = signer();
        assert!(s.verify("!!!not-base64!!!", Purpose::Confirm).is_err());
    }
}
