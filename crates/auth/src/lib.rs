//! Authentication primitives: argon2id hashing, session tokens, CSRF, signed
//! short-lived tokens. No persistence — that belongs to `db`.

pub mod csrf;
pub mod error;
pub mod password;
pub mod session;
pub mod tokens;

pub use error::AuthError;
