//! Argon2id password hashing. Parameters per spec §7: m=64 MiB, t=3, p=4.

use argon2::password_hash::{
    rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString,
};
use argon2::{Algorithm, Argon2, Params, Version};

use crate::AuthError;

const M_KIB: u32 = 64 * 1024; // 64 MiB
const T_COST: u32 = 3;
const P_COST: u32 = 4;

fn argon() -> Argon2<'static> {
    let params = Params::new(M_KIB, T_COST, P_COST, None).expect("static argon2 params");
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
}

pub fn hash(plain: &str) -> Result<String, AuthError> {
    let salt = SaltString::generate(&mut OsRng);
    let phc = argon()
        .hash_password(plain.as_bytes(), &salt)
        .map_err(|e| AuthError::Hash(e.to_string()))?;
    Ok(phc.to_string())
}

pub fn verify(plain: &str, encoded: &str) -> Result<(), AuthError> {
    let parsed = PasswordHash::new(encoded).map_err(|e| AuthError::Hash(e.to_string()))?;
    argon()
        .verify_password(plain.as_bytes(), &parsed)
        .map_err(|_| AuthError::BadPassword)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_then_verify() {
        let h = hash("hunter2").unwrap();
        verify("hunter2", &h).unwrap();
    }

    #[test]
    fn wrong_password_rejected() {
        let h = hash("right").unwrap();
        let err = verify("wrong", &h).unwrap_err();
        assert!(matches!(err, AuthError::BadPassword));
    }

    #[test]
    fn malformed_hash_rejected() {
        let err = verify("any", "not-a-phc-string").unwrap_err();
        assert!(matches!(err, AuthError::Hash(_)));
    }

    #[test]
    fn hashes_differ_for_same_input() {
        // Random salt ensures distinct outputs each invocation.
        let h1 = hash("same").unwrap();
        let h2 = hash("same").unwrap();
        assert_ne!(h1, h2);
    }
}
