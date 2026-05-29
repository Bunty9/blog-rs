//! HMAC-SHA256 signed tokens for member confirm and unsubscribe links.
//! Format: `base64url(payload).base64url(signature)` where payload is a JSON
//! object `{"sub":<member_id>,"purpose":"confirm"|"unsubscribe","exp":<unix>}`.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use time::OffsetDateTime;

use crate::AuthError;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Purpose {
    Confirm,
    Unsubscribe,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Claims {
    pub sub: i64,
    pub purpose: Purpose,
    pub exp: i64,
}

pub fn sign(key: &[u8], sub: i64, purpose: Purpose, ttl_seconds: i64) -> Result<String, AuthError> {
    if key.is_empty() {
        return Err(AuthError::BadKey);
    }
    let claims = Claims {
        sub,
        purpose,
        exp: OffsetDateTime::now_utc().unix_timestamp() + ttl_seconds,
    };
    let payload =
        serde_json::to_vec(&claims).map_err(|e| AuthError::TokenDecode(e.to_string()))?;
    let payload_b64 = URL_SAFE_NO_PAD.encode(&payload);

    let mut mac = HmacSha256::new_from_slice(key).map_err(|_| AuthError::BadKey)?;
    mac.update(payload_b64.as_bytes());
    let sig = URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());

    Ok(format!("{payload_b64}.{sig}"))
}

pub fn verify(key: &[u8], token: &str, expect: Purpose) -> Result<Claims, AuthError> {
    if key.is_empty() {
        return Err(AuthError::BadKey);
    }
    let (payload_b64, sig_b64) = token
        .split_once('.')
        .ok_or_else(|| AuthError::TokenDecode("missing `.` separator".into()))?;

    let expected_sig = {
        let mut mac = HmacSha256::new_from_slice(key).map_err(|_| AuthError::BadKey)?;
        mac.update(payload_b64.as_bytes());
        mac.finalize().into_bytes()
    };
    let provided_sig = URL_SAFE_NO_PAD
        .decode(sig_b64)
        .map_err(|e| AuthError::TokenDecode(e.to_string()))?;
    if provided_sig.len() != expected_sig.len() {
        return Err(AuthError::TokenSignature);
    }
    let mut diff = 0u8;
    for (a, b) in provided_sig.iter().zip(expected_sig.iter()) {
        diff |= a ^ b;
    }
    if diff != 0 {
        return Err(AuthError::TokenSignature);
    }

    let payload = URL_SAFE_NO_PAD
        .decode(payload_b64)
        .map_err(|e| AuthError::TokenDecode(e.to_string()))?;
    let claims: Claims =
        serde_json::from_slice(&payload).map_err(|e| AuthError::TokenDecode(e.to_string()))?;

    if claims.purpose != expect {
        return Err(AuthError::TokenDecode("purpose mismatch".into()));
    }
    if claims.exp <= OffsetDateTime::now_utc().unix_timestamp() {
        return Err(AuthError::TokenExpired);
    }
    Ok(claims)
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &[u8] = b"test-secret-key-do-not-use-in-prod";

    #[test]
    fn round_trip_confirm() {
        let t = sign(KEY, 42, Purpose::Confirm, 60).unwrap();
        let c = verify(KEY, &t, Purpose::Confirm).unwrap();
        assert_eq!(c.sub, 42);
        assert_eq!(c.purpose, Purpose::Confirm);
    }

    #[test]
    fn wrong_purpose_rejected() {
        let t = sign(KEY, 1, Purpose::Confirm, 60).unwrap();
        let err = verify(KEY, &t, Purpose::Unsubscribe).unwrap_err();
        assert!(matches!(err, AuthError::TokenDecode(_)));
    }

    #[test]
    fn wrong_key_rejected() {
        let t = sign(KEY, 1, Purpose::Confirm, 60).unwrap();
        let err = verify(b"different-key-still-long-enough!!", &t, Purpose::Confirm).unwrap_err();
        assert!(matches!(err, AuthError::TokenSignature));
    }

    #[test]
    fn expired_rejected() {
        let t = sign(KEY, 1, Purpose::Confirm, -1).unwrap();
        let err = verify(KEY, &t, Purpose::Confirm).unwrap_err();
        assert!(matches!(err, AuthError::TokenExpired));
    }

    #[test]
    fn tampered_payload_rejected() {
        let t = sign(KEY, 1, Purpose::Confirm, 60).unwrap();
        let (pl, sig) = t.split_once('.').unwrap();
        let mut tampered_pl = URL_SAFE_NO_PAD.decode(pl).unwrap();
        tampered_pl[0] ^= 0x01;
        let bad = format!(
            "{}.{}",
            URL_SAFE_NO_PAD.encode(&tampered_pl),
            sig
        );
        let err = verify(KEY, &bad, Purpose::Confirm).unwrap_err();
        assert!(matches!(err, AuthError::TokenSignature));
    }

    #[test]
    fn empty_key_rejected() {
        assert!(matches!(
            sign(&[], 1, Purpose::Confirm, 60),
            Err(AuthError::BadKey)
        ));
    }
}
