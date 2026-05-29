use thiserror::Error;

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("password hash: {0}")]
    Hash(String),

    #[error("password verify failed")]
    BadPassword,

    #[error("csrf mismatch")]
    CsrfMismatch,

    #[error("token decode: {0}")]
    TokenDecode(String),

    #[error("token expired")]
    TokenExpired,

    #[error("token signature invalid")]
    TokenSignature,

    #[error("hmac key length invalid")]
    BadKey,
}
