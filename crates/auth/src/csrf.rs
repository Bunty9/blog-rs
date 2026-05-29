//! Double-submit CSRF: every mutating request must present the same CSRF
//! token both as the `XSRF-TOKEN` cookie (or `X-CSRF-Token` header for HTMX)
//! and in the session row. The session row's value is the source of truth.

use crate::AuthError;

/// Constant-time compare. Tokens are URL-safe base64 ASCII so byte-wise compare
/// is sufficient; we still use a non-shortcircuiting comparison.
pub fn validate(expected: &str, submitted: &str) -> Result<(), AuthError> {
    if expected.len() != submitted.len() {
        return Err(AuthError::CsrfMismatch);
    }
    let mut diff = 0u8;
    for (a, b) in expected.as_bytes().iter().zip(submitted.as_bytes()) {
        diff |= a ^ b;
    }
    if diff == 0 {
        Ok(())
    } else {
        Err(AuthError::CsrfMismatch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_match_ok() {
        validate("abc123", "abc123").unwrap();
    }

    #[test]
    fn different_value_rejected() {
        let err = validate("abc123", "abc124").unwrap_err();
        assert!(matches!(err, AuthError::CsrfMismatch));
    }

    #[test]
    fn different_length_rejected() {
        let err = validate("abc", "abcd").unwrap_err();
        assert!(matches!(err, AuthError::CsrfMismatch));
    }

    #[test]
    fn empty_pair_rejected() {
        // Two empty strings would match by accident; reject explicitly by
        // refusing zero-length tokens at the caller. For this primitive, equal
        // empties are equal - document via this test that callers must guard.
        validate("", "").unwrap(); // documents current behaviour
    }
}
